use std::collections::HashMap;

use crate::entities::{Entity, EntityData};
use blocks::BlockState;
use chunk::{Chunk, ChunkSection};
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
    values::{Id, Vec2, Vec3},
};

pub mod blocks;
pub mod chunk;
pub mod properties;

#[allow(dead_code)]
#[crate::actor(Dimension, DimensionMessage)]
pub struct DimensionData {
    #[allow(unused)]
    pub(crate) name: Id,
    pub(crate) chunks: HashMap<Vec2<i32>, Chunk>,
    pub(crate) entities: HashMap<Uuid, EntityData>,
    pub(crate) server: Option<Server>,
    pub(crate) sender: Sender<DimensionMessage>,
    pub(crate) dim_type: Id,
    pub(crate) chunk_generator: fn(&mut Chunk, i32, i32),
    pub(crate) chunk_max: (u32, u32),
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
    pub fn name(&self) -> ActorResult<Id> {
        Ok(self.name.clone())
    }

    #[GetServer]
    #[doc = "Get the server this Dimension is running under."]
    pub fn server(&self) -> ActorResult<Server> {
        self.server.clone().ok_or(ActorError::ActorIsNotLoaded)
    }

    #[GetChunkSection]
    #[doc = "Returns a copy of the 16x16x16 chunk section at the provided coordinates."]
    pub fn get_chunk_section(&mut self, position: Vec3<i32>) -> ActorResult<Option<ChunkSection>> {
        let chunk_pos = Vec2::new(position.x(), position.z());
        self.try_initialize_chunk(&chunk_pos)?;

        match self.chunks.get_mut(&chunk_pos) {
            Some(chunk) => {
                let chunk_y = position.y() / 16;
                Ok(Some(chunk.section_at_mut(chunk_y).unwrap().clone()))
            }
            None => Ok(None),
        }
    }

    #[SetBlock]
    #[doc = "Sets a block in this dimension at the given coordinates to the provided block state."]
    pub fn set_block(&mut self, position: Vec3<i32>, block_state: BlockState) -> ActorResult<()> {
        let chunk_pos = Vec2::new(position.x() / 16, position.z() / 16);
        let pos_in_chunk = Vec3::new(position.x() % 16, position.y(), position.z() % 16);

        self.try_initialize_chunk(&chunk_pos)?;

        let Some(chunk) = self.chunks.get_mut(&chunk_pos) else {
            return Ok(());
        };
        chunk.set_block_at(pos_in_chunk, block_state.clone());

        let server = self.server.clone().unwrap();
        Runtime::spawn_task(move || {
            for conn in server.players().unwrap_or_else(|_| Vec::new()) {
                let block_state = block_state.clone();
                let pos = position;
                let conn = conn.clone();

                let _ = conn.write_packet(BlockUpdateS2CPlayPacket {
                    pos: BlockPos::new(pos.x(), pos.y(), pos.z()),
                    block: unsafe { RegEntry::new_unchecked(block_state.protocol_id() as u32) },
                });
            }
            Ok(())
        });
        Ok(())
    }

    #[GetBlock]
    #[doc = "Returns a copy of the block state at the provided coordinates."]
    pub fn get_block(&mut self, position: Vec3<i32>) -> ActorResult<BlockState> {
        let chunk = Vec2::new(position.x() / 16, position.z() / 16);
        let pos_in_chunk = Vec3::new(position.x() % 16, position.y(), position.z() % 16);

        self.try_initialize_chunk(&chunk)?;

        let chunk = self.chunks.get_mut(&chunk).unwrap();
        Ok(chunk.get_block_at(pos_in_chunk))
    }

    #[GetDimType]
    #[doc = "Returns the Dimension Type value of this Dimension."]
    pub fn dimension_type(&mut self) -> ActorResult<Id> {
        Ok(self.dim_type.clone())
    }

    #[SetChunkGenerator]
    #[doc = "Overrides the function that will be called whenever a new Chunk is generated. The default chunk generator is a no-op."]
    pub fn set_chunk_generator(&mut self, function: fn(&mut Chunk, i32, i32)) -> ActorResult<()> {
        self.chunk_generator = function;
        Ok(())
    }

    #[GetAllEntities]
    #[doc = "Returns a handle to all of the entities present in this dimension."]
    pub fn entities(&self) -> ActorResult<Vec<Entity>> {
        Ok(self
            .entities
            .values()
            .filter(|x| x.entity_type != Id::constant("minecraft", "player"))
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
    pub fn all_entities(&self) -> ActorResult<Vec<Entity>> {
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
    pub fn spawn_entity(&mut self, entity_type: Id) -> ActorResult<Entity> {
        let mut uuid = Uuid::new_v4();
        while self.entities.contains_key(&uuid) {
            uuid = Uuid::new_v4();
        }

        let id = self.server.clone().unwrap().new_entity_id()?;

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

        Runtime::spawn_task(move || {
            for conn in dim.players().unwrap_or_else(|_| Vec::new()) {
                let conn = dim.server().unwrap().player(conn).unwrap();
                let _ = conn.write_packet(AddEntityS2CPlayPacket {
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
                });
            }

            Ok(())
        });

        Ok(Entity {
            dimension: Dimension {
                sender: self.sender.clone(),
            },
            uuid,
        })
    }

    #[SpawnPlayerEntity]
    pub(crate) fn spawn_player_entity(&mut self, uuid: Uuid, id: i32) -> ActorResult<Entity> {
        self.entities.insert(uuid, EntityData {
            entity_type: Id::constant("minecraft", "player"),
            uuid,
            id,
            position: Vec3::new(0.0, 0.0, 0.0),
            heading: Vec2::new(0.0, 0.0),
            metadata: EntityMetadata::new(),
        });

        let dim = Dimension {
            sender: self.sender.clone(),
        };

        Runtime::spawn_task(move || {
            for conn in dim.players().unwrap_or_else(|_| Vec::new()) {
                if conn != uuid {
                    let conn = dim.server().unwrap().player(conn).unwrap();
                    let _ = conn.write_packet(AddEntityS2CPlayPacket {
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
                    });
                }
            }
            Ok(())
        });

        Ok(Entity {
            dimension: Dimension {
                sender: self.sender.clone(),
            },
            uuid,
        })
    }

    #[RemoveEntity]
    pub(crate) fn remove_entity(&mut self, uuid: Uuid) -> ActorResult<()> {
        let entry = self.entities.remove(&uuid);

        if let Some(entry) = entry {
            let server = self
                .server
                .as_ref()
                .ok_or(ActorError::ActorDoesNotExist)?
                .clone();

            Runtime::spawn_task(move || {
                for conn in server.connections().unwrap() {
                    let _ = conn.write_packet(RemoveEntitiesS2CPlayPacket {
                        entities: vec![VarInt::new(entry.id)].into(),
                    });
                }
                Ok(())
            });
        };

        Ok(())
    }

    #[EntityId]
    pub(crate) fn entity_id(&mut self, uuid: Uuid) -> ActorResult<i32> {
        self.entities
            .get(&uuid)
            .ok_or(ActorError::ActorDoesNotExist)
            .map(|x| x.id)
    }

    #[EntityType]
    pub(crate) fn entity_type(&mut self, uuid: Uuid) -> ActorResult<Id> {
        self.entities
            .get(&uuid)
            .ok_or(ActorError::ActorDoesNotExist)
            .map(|x| x.entity_type.clone())
    }

    #[EntityPos]
    pub(crate) fn entity_pos(&mut self, uuid: Uuid) -> ActorResult<(Vec3<f64>, Vec2<f32>)> {
        self.entities
            .get(&uuid)
            .ok_or(ActorError::ActorDoesNotExist)
            .map(|x| (x.position, x.heading))
    }

    #[TeleportEntity]
    pub(crate) fn teleport_entity(&mut self, uuid: Uuid, position: Vec3<f64>) -> ActorResult<()> {
        if let Some(entity) = self.entities.get_mut(&uuid) {
            entity.position = position;
            let entity = entity.clone();

            let dim = Dimension {
                sender: self.sender.clone(),
            };

            Runtime::spawn_task(move || {
                for conn in dim.players().unwrap() {
                    if conn != entity.uuid {
                        let conn = dim.server().unwrap().player(conn).unwrap();
                        let _ = conn.write_packet(EntityPositionSyncS2CPlayPacket {
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
                        });
                    }
                }
                Ok(())
            });
        }
        Ok(())
    }

    #[RotateEntity]
    pub(crate) fn rotate_entity(&mut self, uuid: Uuid, heading: Vec2<f32>) -> ActorResult<()> {
        if let Some(entity) = self.entities.get_mut(&uuid) {
            entity.heading = heading;
            let entity = entity.clone();
            let dim = Dimension {
                sender: self.sender.clone(),
            };

            Runtime::spawn_task(move || {
                for conn in dim.players().unwrap() {
                    if conn != entity.uuid {
                        let conn = dim.server().unwrap().player(conn).unwrap();
                        let _ = conn.write_packet(EntityPositionSyncS2CPlayPacket {
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
                        });
                    }
                }
                Ok(())
            });
        }
        Ok(())
    }

    #[GetPlayers]
    #[doc = "Returns the UUID for all players present in this dimension."]
    pub fn players(&mut self) -> ActorResult<Vec<Uuid>> {
        let mut vec = Vec::new();
        for entity in &mut self.entities {
            if entity.1.entity_type == Id::constant("minecraft", "player") {
                vec.push(entity.1.uuid);
            }
        }
        Ok(vec)
    }

    #[SetChunkLimits]
    #[doc = "Sets the maximum number of chunks this dimension can hold."]
    pub fn max_chunks(&mut self, x: u32, y: u32) -> ActorResult<()> {
        self.chunk_max = (x, y);
        Ok(())
    }
}

impl DimensionData {
    pub(crate) fn new(name: Id, server: Server, dim_type: Id) -> DimensionData {
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
            chunk_max: (i32::MAX as u32, i32::MAX as u32),
        }
    }

    pub(crate) fn try_initialize_chunk(&mut self, pos: &Vec2<i32>) -> ActorResult<()> {
        if !self.chunks.contains_key(pos)
            && pos.x() <= self.chunk_max.0 as i32
            && pos.y() <= self.chunk_max.1 as i32
        {
            let server = self.server.clone().unwrap();
            let registries = server.registries()?;

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
