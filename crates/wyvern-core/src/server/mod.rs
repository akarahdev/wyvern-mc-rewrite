use std::{
    net::{Ipv4Addr, SocketAddrV4},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use crate::{
    actor,
    actors::{ActorError, ActorResult},
    message,
    player::PlayerComponents,
};
use crate::{actors::Actor, runtime::Runtime};
use dimensions::DimensionContainer;
use flume::Sender;
use registries::RegistryContainer;
use voxidian_protocol::{packet::Stage, value::Uuid};
use wyvern_textures::TexturePack;

use crate::{
    dimension::{Dimension, DimensionData},
    events::{DimensionCreateEvent, Event, EventBus, ServerStartEvent, ServerTickEvent},
    player::{ConnectionData, ConnectionWithSignal, Player},
};
use wyvern_values::Id;

mod builder;
pub use builder::*;
pub mod dimensions;
pub mod registries;

static SERVER_INSTANCE: OnceLock<Server> = OnceLock::new();

#[actor(Server, ServerMessage)]
pub(crate) struct ServerData {
    pub(crate) connections: Vec<ConnectionWithSignal>,
    pub(crate) registries: Arc<RegistryContainer>,
    pub(crate) dimensions: DimensionContainer,
    pub(crate) last_tick: Instant,
    pub(crate) sender: Sender<ServerMessage>,
    pub(crate) events: Arc<EventBus>,
    pub(crate) last_entity_id: i32,
    pub(crate) mojauth_enabled: bool,
    pub(crate) texture_pack: Option<Arc<TexturePack>>,
    pub(crate) default_dimension: Id,
}

impl Server {
    pub fn get() -> ActorResult<Server> {
        SERVER_INSTANCE
            .get()
            .ok_or(ActorError::ActorDoesNotExist)
            .cloned()
    }

    pub fn spawn_event<E: Event + Send + Sync + 'static>(&self, event: E) -> ActorResult<()> {
        let server = self.clone();
        Runtime::spawn_task(async move {
            event.dispatch(server.event_bus().unwrap());
            Ok(())
        });
        Ok(())
    }
}

#[message(Server, ServerMessage)]
impl ServerData {
    #[DefaultDimension]
    pub fn default_dimension(&self) -> ActorResult<Id> {
        Ok(self.default_dimension.clone())
    }

    #[SetDefaultDimension]
    pub fn set_default_dimension(&mut self, id: Id) -> ActorResult<()> {
        self.default_dimension = id;
        Ok(())
    }

    #[ResourcePack]
    pub fn resource_pack(&self) -> ActorResult<Arc<TexturePack>> {
        self.texture_pack.clone().ok_or(ActorError::BadRequest)
    }

    #[MojauthEnabled]
    pub fn mojauth_enabled(&self) -> ActorResult<bool> {
        Ok(self.mojauth_enabled)
    }

    #[NewEntityId]
    pub fn new_entity_id(&mut self) -> ActorResult<i32> {
        self.last_entity_id += 1;
        log::debug!("New entity id produced: {:?}", self.last_entity_id);
        Ok(self.last_entity_id)
    }

    #[GetEventBus]
    pub fn event_bus(&mut self) -> ActorResult<Arc<EventBus>> {
        Ok(self.events.clone())
    }

    #[SpawnConnectionInternal]
    pub fn spawn_connection_internal(&mut self, conn: ConnectionWithSignal) -> ActorResult<()> {
        self.connections.push(conn);
        Ok(())
    }

    #[GetRegistries]
    pub fn registries(&self) -> ActorResult<Arc<RegistryContainer>> {
        Ok(self.registries.clone())
    }

    #[GetDimension]
    pub fn dimension(&self, key: Id) -> ActorResult<Dimension> {
        self.dimensions
            .get(&key)
            .cloned()
            .ok_or(ActorError::IndexOutOfBounds)
    }

    #[GetAllDimensions]
    pub fn dimensions(&self) -> ActorResult<Vec<Dimension>> {
        Ok(self.dimensions.dimensions().cloned().collect())
    }

    #[CreateDimension]
    pub fn create_dimension(&mut self, name: Id, dim_type: Id) -> ActorResult<Dimension> {
        log::debug!("Creating new dimension: {:?}", name);
        let root_dim = DimensionData::new(name.clone(), self.as_actor(), dim_type);

        let dim = Dimension {
            sender: root_dim.sender.downgrade(),
        };
        self.dimensions.insert(name, dim.clone());
        Runtime::spawn_actor(
            move || {
                root_dim.event_loop();
            },
            "DimensionThread",
        );

        let dim_clone = dim.clone();
        let server_clone = self.as_actor();
        let _ = server_clone.spawn_event(DimensionCreateEvent {
            dimension: dim_clone,
            server: server_clone.clone(),
        });

        Ok(dim)
    }

    #[GetConnections]
    pub fn connections(&self) -> ActorResult<Vec<Player>> {
        Ok(self.connections.iter().map(|x| x.lower()).collect())
    }

    #[GetPlayers]
    pub fn players(&mut self) -> ActorResult<Vec<Player>> {
        let mut vec = Vec::new();

        for conn in &self.connections {
            let stage = *conn.stage.lock().unwrap() == Stage::Play;

            if stage {
                vec.push(conn.lower());
            }
        }
        Ok(vec)
    }

    #[GetPlayerByUuid]
    pub fn player(&self, player: Uuid) -> ActorResult<Player> {
        for conn in &self.connections {
            if conn.player.get(PlayerComponents::UUID) == Ok(player)
                && conn.player.stage() == Ok(Stage::Play)
            {
                return Ok(conn.player.clone());
            }
        }
        Err(ActorError::BadRequest)
    }
}

impl ServerData {
    pub fn start(self) {
        log::info!("A server is starting!");
        let snd = self.as_actor();

        if let Some(pack) = self.texture_pack.clone() {
            std::thread::spawn(move || {
                pack.host();
            });
        }

        SERVER_INSTANCE.set(snd.clone()).unwrap_or_else(|_| {
            log::error!("WyvernMC does not support running two servers at once. Bugs may occur.");
        });
        let snd_clone = snd.clone();
        Runtime::spawn_task(async move {
            snd_clone
                .spawn_event(ServerStartEvent {
                    server: snd_clone.clone(),
                })
                .unwrap();
            Ok(())
        });
        let snd_clone = snd.clone();

        Runtime::spawn_actor(
            move || Self::networking_loop(snd_clone),
            "ServerNetworkingThread",
        );
        self.handle_loops(snd);
    }

    pub fn handle_loops(mut self, server: Server) {
        loop {
            self.connections
                .retain_mut(|connection| connection._signal.try_recv().is_err());

            self.handle_messages();
            let dur = Instant::now().duration_since(self.last_tick);
            if dur > Duration::from_millis(50) {
                self.last_tick = Instant::now();

                let _ = server.spawn_event(ServerTickEvent {
                    server: server.clone(),
                });

                for c in self.connections.iter() {
                    let mut player = c.lower();
                    if c.stage.lock().map(|x| *x).unwrap_or(Stage::Handshake) == Stage::Play {
                        Runtime::spawn_task(async move { player.update_components() });
                    }
                }
            }
        }
    }

    pub fn networking_loop(server: Server) {
        let listener =
            std::net::TcpListener::bind(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 25565))
                .unwrap();

        log::info!("A server is now listening on: 127.0.0.1:25565");
        loop {
            let new_client = listener.accept();
            match new_client {
                Ok((stream, addr)) => {
                    log::info!("Accepted new client: {:?}", addr);
                    let stage = Arc::new(Mutex::new(Stage::Handshake));
                    let signal = ConnectionData::connection_channel(
                        stream,
                        addr.ip(),
                        server.clone(),
                        stage,
                    );
                    let _ = server.spawn_connection_internal(signal);
                }
                Err(_err) => {}
            }
        }
    }
}

impl Server {
    pub fn builder() -> ServerBuilder {
        ServerBuilder::new()
    }
}
