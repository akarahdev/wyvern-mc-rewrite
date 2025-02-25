use std::collections::HashMap;

use blocks::BlockState;
use chunk::{Chunk, ChunkSection};
use entity::{Entity, EntityData, EntityType};
use flume::Sender;
use voxidian_protocol::{
    packet::s2c::play::{
        AddEntityS2CPlayPacket, BlockUpdateS2CPlayPacket, EntityPositionSyncS2CPlayPacket,
        RemoveEntitiesS2CPlayPacket,
    },
    registry::RegEntry,
    value::{
        Angle, BlockPos, EntityMetadata, EntityType as PtcEntityType, Identifier, Uuid, VarInt,
    },
};

use crate::{
    actors::{ActorError, ActorResult},
    events::ChunkLoadEvent,
    runtime::Runtime,
    server::Server,
    values::{Key, Vec2, Vec3, regval::DimensionType},
};

pub mod blocks;
pub mod chunk;
pub mod entity;
pub mod properties;

#[allow(dead_code)]
#[crate::actor(Dimension, DimensionMessage)]
pub struct DimensionData {
    #[allow(unused)]
    pub(crate) name: Key<DimensionData>,
    pub(crate) chunks: HashMap<Vec2<i32>, Chunk>,
    pub(crate) entities: HashMap<Uuid, EntityData>,
    pub(crate) server: Option<Server>,
    pub(crate) sender: Sender<DimensionMessage>,
    pub(crate) dim_type: Key<DimensionType>,
    pub(crate) chunk_generator: fn(&mut Chunk, i32, i32),
}

impl Dimension {
    pub fn get_entity(&self, entity: Uuid) -> Entity {
        Entity {
            uuid: entity,
            dimension: self.clone(),
        }
    }
}

#[crate::message(Dimension, DimensionMessage)]
impl DimensionData {
    #[GetName]
    #[doc = "Get the name of this dimension."]
    pub async fn name(&self) -> ActorResult<Key<Dimension>> {
        Ok(self.name.clone().retype())
    }

    #[GetServer]
    #[doc = "Get the server this Dimension is running under."]
    pub async fn server(&self) -> ActorResult<Server> {
        self.server.clone().ok_or(ActorError::ActorIsNotLoaded)
    }

    #[GetChunkSection]
    #[doc = "Returns a copy of the 16x16x16 chunk section at the provided coordinates."]
    pub async fn get_chunk_section(&mut self, position: Vec3<i32>) -> ActorResult<ChunkSection> {
        let chunk_pos = Vec2::new(position.x(), position.z());
        self.try_initialize_chunk(&chunk_pos).await?;

        let chunk = self.chunks.get_mut(&chunk_pos).unwrap();
        let chunk_y = position.y() / 16;
        Ok(chunk.section_at_mut(chunk_y).unwrap().clone())
    }

    #[SetBlock]
    #[doc = "Sets a block in this dimension at the given coordinates to the provided block state."]
    pub async fn set_block(
        &mut self,
        position: Vec3<i32>,
        block_state: BlockState,
    ) -> ActorResult<()> {
        let chunk_pos = Vec2::new(position.x() / 16, position.z() / 16);
        let pos_in_chunk = Vec3::new(position.x() % 16, position.y(), position.z() % 16);

        self.try_initialize_chunk(&chunk_pos).await?;

        let chunk = self.chunks.get_mut(&chunk_pos).unwrap();
        chunk.set_block_at(pos_in_chunk, block_state.clone());

        let server = self.server.clone().unwrap();
        Runtime::spawn(async move {
            for conn in server.players().await.unwrap_or_else(|_| Vec::new()) {
                let block_state = block_state.clone();
                let pos = position;
                let conn = conn.clone();

                let _ = conn
                    .write_packet(BlockUpdateS2CPlayPacket {
                        pos: BlockPos::new(pos.x(), pos.y(), pos.z()),
                        block: unsafe { RegEntry::new_unchecked(block_state.protocol_id() as u32) },
                    })
                    .await;
            }
        });
        Runtime::yield_now().await;
        Ok(())
    }

    #[GetBlock]
    #[doc = "Returns a copy of the block state at the provided coordinates."]
    pub async fn get_block(&mut self, position: Vec3<i32>) -> ActorResult<BlockState> {
        let chunk = Vec2::new(position.x() / 16, position.z() / 16);
        let pos_in_chunk = Vec3::new(position.x() % 16, position.y(), position.z() % 16);

        self.try_initialize_chunk(&chunk).await?;

        let chunk = self.chunks.get_mut(&chunk).unwrap();
        Ok(chunk.get_block_at(pos_in_chunk))
    }

    #[GetDimType]
    #[doc = "Returns the Dimension Type value of this Dimension."]
    pub async fn dimension_type(&mut self) -> ActorResult<Key<DimensionType>> {
        Ok(self.dim_type.clone())
    }

    #[SetChunkGenerator]
    #[doc = "Overrides the function that will be called whenever a new Chunk is generated. The default chunk generator is a no-op."]
    pub async fn set_chunk_generator(
        &mut self,
        function: fn(&mut Chunk, i32, i32),
    ) -> ActorResult<()> {
        self.chunk_generator = function;
        Ok(())
    }

    #[GetAllEntities]
    #[doc = "Returns a handle to all of the entities present in this dimension."]
    pub async fn entities(&self) -> ActorResult<Vec<Entity>> {
        Ok(self
            .entities
            .values()
            .filter(|x| x.entity_type != Key::constant("minecraft", "player"))
            .map(|x| Entity {
                dimension: Dimension {
                    sender: self.sender.clone(),
                },
                uuid: x.uuid,
            })
            .collect())
    }

    #[GetAllEntitiesAndHumans]
    #[doc = "Returns a handle to all of the entities present in this dimension, including human entities."]
    pub async fn all_entities(&self) -> ActorResult<Vec<Entity>> {
        Ok(self
            .entities
            .values()
            .map(|x| Entity {
                dimension: Dimension {
                    sender: self.sender.clone(),
                },
                uuid: x.uuid,
            })
            .collect())
    }

    #[SpawnEntity]
    #[doc = "Spawns a new entity in the dimension with the given type, returning a handle to the entity."]
    pub async fn spawn_entity(&mut self, entity_type: Key<EntityType>) -> ActorResult<Entity> {
        let mut uuid = Uuid::new_v4();
        while self.entities.contains_key(&uuid) {
            uuid = Uuid::new_v4();
        }

        let id = self.server.clone().unwrap().new_entity_id().await?;

        self.entities.insert(uuid, EntityData {
            entity_type: entity_type.clone(),
            uuid,
            id,
            position: Vec3::new(0.0, 0.0, 0.0),
            heading: Vec2::new(0.0, 0.0),
            metadata: EntityMetadata::new(),
        });

        let dim = Dimension {
            sender: self.sender.clone(),
        };

        Runtime::spawn(async move {
            for conn in dim.players().await.unwrap_or_else(|_| Vec::new()) {
                let conn = dim.server().await.unwrap().player(conn).await.unwrap();
                let _ = conn
                    .write_packet(AddEntityS2CPlayPacket {
                        id: id.into(),
                        uuid,
                        kind: PtcEntityType::vanilla_registry()
                            .get_entry(&entity_type.clone().into())
                            .unwrap(),
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        pitch: Angle::of_deg(0.0),
                        yaw: Angle::of_deg(0.0),
                        head_yaw: Angle::of_deg(0.0),
                        data: VarInt::from(0),
                        vel_x: 0,
                        vel_y: 0,
                        vel_z: 0,
                    })
                    .await;
            }
        });

        Ok(Entity {
            dimension: Dimension {
                sender: self.sender.clone(),
            },
            uuid,
        })
    }

    #[SpawnPlayerEntity]
    pub(crate) async fn spawn_player_entity(&mut self, uuid: Uuid, id: i32) -> ActorResult<Entity> {
        self.entities.insert(uuid, EntityData {
            entity_type: Key::constant("minecraft", "player"),
            uuid,
            id,
            position: Vec3::new(0.0, 0.0, 0.0),
            heading: Vec2::new(0.0, 0.0),
            metadata: EntityMetadata::new(),
        });

        let dim = Dimension {
            sender: self.sender.clone(),
        };

        Runtime::spawn(async move {
            for conn in dim.players().await.unwrap_or_else(|_| Vec::new()) {
                if conn != uuid {
                    let conn = dim.server().await.unwrap().player(conn).await.unwrap();
                    let _ = conn
                        .write_packet(AddEntityS2CPlayPacket {
                            id: id.into(),
                            uuid,
                            kind: PtcEntityType::vanilla_registry()
                                .get_entry(&Identifier::new("minecraft", "player"))
                                .unwrap(),
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                            pitch: Angle::of_deg(0.0),
                            yaw: Angle::of_deg(0.0),
                            head_yaw: Angle::of_deg(0.0),
                            data: VarInt::from(0),
                            vel_x: 0,
                            vel_y: 0,
                            vel_z: 0,
                        })
                        .await;
                }
            }
        });

        Ok(Entity {
            dimension: Dimension {
                sender: self.sender.clone(),
            },
            uuid,
        })
    }

    #[RemoveEntity]
    pub(crate) async fn remove_entity(&mut self, uuid: Uuid) -> ActorResult<()> {
        let entry = self.entities.remove(&uuid);

        if let Some(entry) = entry {
            let server = self
                .server
                .as_ref()
                .ok_or(ActorError::ActorDoesNotExist)?
                .clone();

            Runtime::spawn(async move {
                for conn in server.connections().await.unwrap() {
                    let _ = conn
                        .write_packet(RemoveEntitiesS2CPlayPacket {
                            entities: vec![VarInt::new(entry.id)].into(),
                        })
                        .await;
                }
            });
        };

        Ok(())
    }

    #[EntityId]
    pub(crate) async fn entity_id(&mut self, uuid: Uuid) -> ActorResult<i32> {
        self.entities
            .get(&uuid)
            .ok_or(ActorError::ActorDoesNotExist)
            .map(|x| x.id)
    }

    #[EntityType]
    pub(crate) async fn entity_type(&mut self, uuid: Uuid) -> ActorResult<Key<EntityType>> {
        self.entities
            .get(&uuid)
            .ok_or(ActorError::ActorDoesNotExist)
            .map(|x| x.entity_type.clone())
    }

    #[EntityPos]
    pub(crate) async fn entity_pos(&mut self, uuid: Uuid) -> ActorResult<(Vec3<f64>, Vec2<f32>)> {
        self.entities
            .get(&uuid)
            .ok_or(ActorError::ActorDoesNotExist)
            .map(|x| (x.position, x.heading))
    }

    #[TeleportEntity]
    pub(crate) async fn teleport_entity(
        &mut self,
        uuid: Uuid,
        position: Vec3<f64>,
    ) -> ActorResult<()> {
        if let Some(entity) = self.entities.get_mut(&uuid) {
            entity.position = position;
            let entity = entity.clone();

            let dim = Dimension {
                sender: self.sender.clone(),
            };

            Runtime::spawn(async move {
                for conn in dim.players().await.unwrap() {
                    if conn != entity.uuid {
                        let conn = dim.server().await.unwrap().player(conn).await.unwrap();
                        let _ = conn
                            .write_packet(EntityPositionSyncS2CPlayPacket {
                                entity_id: entity.id.into(),
                                x: entity.position.x(),
                                y: entity.position.y(),
                                z: entity.position.z(),
                                vx: 0.0,
                                vy: 0.0,
                                vz: 0.0,
                                yaw: entity.heading.x(),
                                pitch: entity.heading.y(),
                                on_ground: false,
                            })
                            .await;
                    }
                }
            });
        }
        Ok(())
    }

    #[RotateEntity]
    pub(crate) async fn rotate_entity(
        &mut self,
        uuid: Uuid,
        heading: Vec2<f32>,
    ) -> ActorResult<()> {
        if let Some(entity) = self.entities.get_mut(&uuid) {
            entity.heading = heading;
            let entity = entity.clone();
            let dim = Dimension {
                sender: self.sender.clone(),
            };

            Runtime::spawn(async move {
                for conn in dim.players().await.unwrap() {
                    if conn != entity.uuid {
                        let conn = dim.server().await.unwrap().player(conn).await.unwrap();
                        let _ = conn
                            .write_packet(EntityPositionSyncS2CPlayPacket {
                                entity_id: entity.id.into(),
                                x: entity.position.x(),
                                y: entity.position.y(),
                                z: entity.position.z(),
                                vx: 0.0,
                                vy: 0.0,
                                vz: 0.0,
                                yaw: entity.heading.x(),
                                pitch: entity.heading.y(),
                                on_ground: false,
                            })
                            .await;
                    }
                }
            });
        }
        Ok(())
    }

    #[GetPlayers]
    #[doc = "Returns the UUID for all players present in this dimension."]
    pub async fn players(&mut self) -> ActorResult<Vec<Uuid>> {
        let mut vec = Vec::new();
        for entity in &mut self.entities {
            if entity.1.entity_type == Key::constant("minecraft", "player") {
                vec.push(entity.1.uuid);
            }
        }
        Ok(vec)
    }
}

impl DimensionData {
    pub(crate) fn new(
        name: Key<DimensionData>,
        server: Server,
        dim_type: Key<DimensionType>,
    ) -> DimensionData {
        let chan = flume::unbounded();
        DimensionData {
            name,
            chunks: HashMap::new(),
            entities: HashMap::new(),
            server: Some(server),
            receiver: chan.1,
            sender: chan.0,
            dim_type,
            chunk_generator: |_, _, _| {},
        }
    }

    pub(crate) async fn try_initialize_chunk(&mut self, pos: &Vec2<i32>) -> ActorResult<()> {
        if !self.chunks.contains_key(pos) {
            let server = self.server.clone().unwrap();
            let registries = server.registries().await?;

            let dim_type = registries
                .dimension_types
                .get(self.dim_type.clone())
                .unwrap();

            let min_sections = dim_type.min_y / 16;
            let max_sections = (dim_type.min_y + dim_type.height as i32) / 16;

            let mut chunk = Chunk::new(min_sections, max_sections);
            (self.chunk_generator)(&mut chunk, pos.x(), pos.y());
            self.chunks.insert(*pos, chunk);

            let sender = self.sender.clone();
            server.spawn_event(ChunkLoadEvent {
                dimension: Dimension { sender },
                pos: *pos,
            })?;
        }
        Ok(())
    }
}
