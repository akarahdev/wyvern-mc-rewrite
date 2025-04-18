use std::{collections::HashMap, sync::LazyLock};

use voxidian_protocol::{
    packet::s2c::play::ChunkBlockEntity,
    registry::{RegEntry, Registry},
    value::{
        ChunkSection as ProtocolSection, PaletteFormat, PalettedContainer, RawDataArray, VarInt,
    },
};
use wyvern_components::DataComponentHolder;
use wyvern_datatypes::nbt::Nbt;

use crate::{
    blocks::BlockComponents,
    server::{Server, registries::RegistryKeys},
};

use wyvern_values::{I16Vec3, IVec3, Id, USizeVec3};

use crate::blocks::BlockState;

pub static BLOCK_ENTITY_REGISTRY: LazyLock<Registry<VarInt>> =
    LazyLock::new(ChunkBlockEntity::block_entity_type_registry);

#[derive(Clone, Debug)]
pub struct Chunk {
    pub(crate) min_sections: i32,
    pub(crate) _max_sections: i32,
    pub(crate) sections: Vec<ChunkSection>,
    pub(crate) block_entities: HashMap<I16Vec3, VarInt>,
}

impl Chunk {
    pub fn new(min_sections: i32, max_sections: i32) -> Chunk {
        let total_sections = max_sections + -min_sections;
        let mut vec = Vec::with_capacity(total_sections as usize);
        for _ in 0..total_sections {
            vec.push(ChunkSection::empty());
        }
        Chunk {
            min_sections,
            _max_sections: max_sections,
            sections: vec,
            block_entities: HashMap::new(),
        }
    }

    pub(crate) fn section_at_mut(&mut self, section: i32) -> Option<&mut ChunkSection> {
        self.sections
            .get_mut((section - self.min_sections) as usize)
    }

    pub fn set_block_at(&mut self, pos: IVec3, block: &BlockState) {
        let section_y = pos[1].div_euclid(16);
        let local_y = pos[1].rem_euclid(16);
        let name = block.name().clone();
        if let Some(section) = self.section_at_mut(section_y) {
            section.set_block_at(pos.with_y(local_y).as_usizevec3(), block);

            if let Some(id) = BLOCK_ENTITY_REGISTRY.get(&name.into()) {
                self.block_entities.insert(pos.as_i16vec3(), *id);
            } else {
                self.block_entities.remove(&pos.as_i16vec3());
            }
        }
    }

    pub fn set_block_at_by_id(&mut self, pos: IVec3, block: u32) {
        let section_y = pos[1].div_euclid(16);
        let local_y = pos[1].rem_euclid(16);
        if let Some(section) = self.section_at_mut(section_y) {
            section.set_block_at_by_id(pos.with_y(local_y).as_usizevec3(), block);

            // TODO: add this
            // if let Some(id) = BLOCK_ENTITY_REGISTRY.get(&name) {
            //     self.block_entities.insert(pos.as_i16vec3(), *id);
            // } else {
            //     self.block_entities.remove(&pos.as_i16vec3());
            // }
        }
    }

    pub fn get_block_at(&mut self, pos: IVec3) -> BlockState {
        let section_y = pos[1].div_euclid(16);
        let local_y = pos[1].rem_euclid(16);

        if let Some(section) = self.section_at_mut(section_y) {
            section.get_block_at(pos.as_usizevec3().with_y(local_y as usize))
        } else {
            BlockState::from_protocol_id(0)
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ChunkSection {
    block_count: i16,
    blocks: RawDataArray,
    block_meta: HashMap<USizeVec3, Nbt>,
}

impl ChunkSection {
    pub fn index_from_pos(pos: USizeVec3) -> usize {
        pos[1] * 256 + pos[2] * 16 + pos[0]
    }

    pub fn empty() -> ChunkSection {
        ChunkSection {
            block_count: 0,
            blocks: {
                let mut arr = RawDataArray::new(15);
                for _ in 0..4096 {
                    arr.push(0);
                }
                arr
            },
            block_meta: HashMap::new(),
        }
    }

    pub fn set_block_at(&mut self, pos: USizeVec3, block: &BlockState) {
        let idx = Self::index_from_pos(pos);
        let old_block = self.blocks.get(idx).unwrap();

        let new_block =
            unsafe { RegEntry::<BlockState>::new_unchecked(block.protocol_id() as u32) }.id();

        if old_block == 0 && new_block != 0 {
            self.block_count += 1;
        } else if old_block != 0 && new_block == 0 {
            self.block_count -= 1;
        }

        self.blocks.set(idx, new_block as u64);
        if let Ok(data) = block.get(BlockComponents::CUSTOM_DATA) {
            self.block_meta.insert(pos, data);
        }
    }

    pub fn set_block_at_by_id(&mut self, pos: USizeVec3, new_block: u32) {
        let idx = Self::index_from_pos(pos);
        let old_block = self.blocks.get(idx).unwrap();

        if old_block == 0 && new_block != 0 {
            self.block_count += 1;
        } else if old_block != 0 && new_block == 0 {
            self.block_count -= 1;
        }

        self.blocks.set(idx, new_block as u64);
    }

    pub fn get_block_at(&mut self, pos: USizeVec3) -> BlockState {
        let ptc = self.blocks.get(Self::index_from_pos(pos)).unwrap();
        let mut state = BlockState::from_protocol_id(ptc as i32);

        if let Some(cdata) = self.block_meta.get(&pos) {
            state.set(BlockComponents::CUSTOM_DATA, cdata.clone());
        }

        state
    }

    pub fn as_protocol_section(&self) -> ProtocolSection {
        ProtocolSection {
            block_count: self.block_count,
            block_states: PalettedContainer {
                bits_per_entry: 15,
                format: PaletteFormat::RawDirect {
                    data: self.blocks.clone(),
                },
            },
            biomes: PalettedContainer {
                bits_per_entry: 0,
                format: PaletteFormat::SingleValued {
                    entry: Server::get()
                        .unwrap()
                        .registries()
                        .unwrap()
                        .get(RegistryKeys::BIOME)
                        .get_entry(Id::new("minecraft", "plains"))
                        .unwrap(),
                },
            },
        }
    }
}
