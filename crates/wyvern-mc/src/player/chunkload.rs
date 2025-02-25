use std::time::Instant;

use voxidian_protocol::{
    packet::s2c::play::{
        ChunkBatchFinishedS2CPlayPacket, ChunkBatchStartS2CPlayPacket,
        LevelChunkWithLightS2CPlayPacket, SetChunkCacheCenterS2CPlayPacket,
    },
    value::{ChunkSectionData, Nbt, NbtCompound, VarInt},
};

use crate::{
    runtime::Runtime,
    values::{Vec2, Vec3},
};

use super::{ConnectionData, Player};

impl ConnectionData {
    pub fn send_chunks(&mut self) {
        let Some(dimension) = self.associated_data.dimension.clone() else {
            return;
        };

        let chunk_center = Vec2::new(
            f64::floor(self.associated_data.last_position.x() / 16.0) as i32,
            f64::floor(self.associated_data.last_position.z() / 16.0) as i32,
        );

        self.associated_data.last_chunk_position = chunk_center;

        let cx = chunk_center.x();
        let cz = chunk_center.y();

        let render_distance = (self.associated_data.render_distance / 2) + 2;

        self.associated_data.loaded_chunks = self
            .associated_data
            .loaded_chunks
            .iter()
            .filter(|position| {
                position.x() >= cx - render_distance
                    && position.x() <= cx + render_distance
                    && position.y() >= cz - render_distance
                    && position.y() <= cz + render_distance
            })
            .copied()
            .collect::<Vec<_>>();

        let mut chunks = Vec::new();
        for chunk_x in (cx - render_distance)..(cx + render_distance) {
            for chunk_z in (cz - render_distance)..(cz + render_distance) {
                let pos = Vec2::new(chunk_x, chunk_z);
                if !self.associated_data.loaded_chunks.contains(&pos) {
                    chunks.push(pos);
                }
            }
        }

        chunks.sort_by(|lhs, rhs| {
            let lhs_dist = i32::isqrt(i32::pow(lhs.x() - cx, 2) + i32::pow(lhs.y() - cz, 2));
            let rhs_dist = i32::isqrt(i32::pow(rhs.x() - cx, 2) + i32::pow(rhs.y() - cz, 2));
            lhs_dist.cmp(&rhs_dist)
        });

        let player = Player {
            sender: self.sender.upgrade().unwrap(),
        };
        let server = self.connected_server.clone();

        if let Some(pos) = chunks.first() {
            let pos = *pos;
            self.associated_data.loaded_chunks.push(pos);
            Runtime::spawn(async move {
                let dim_type_entry = dimension.dimension_type().await.unwrap();

                let (min_y, max_y, height) = {
                    let registries = server.registries().await.unwrap();
                    let dim_type = registries
                        .dimension_types
                        .get(dim_type_entry.retype())
                        .unwrap();

                    let min_y = dim_type.min_y;
                    let max_y = dim_type.min_y + dim_type.height as i32;
                    let height = dim_type.height;

                    (min_y, max_y, height)
                };

                log::error!(
                    "Player {:?} is loading chunk @ {:?}",
                    player.username().await,
                    pos
                );
                let chunk_x = pos.x();
                let chunk_z = pos.y();

                let start = Instant::now();
                let mut sections = Vec::new();
                for y in (min_y..max_y).step_by(16) {
                    let pos = Vec3::new(chunk_x, y, chunk_z);
                    let chunk = dimension.get_chunk_section(pos).await.unwrap();
                    sections.push(chunk.as_protocol_section());
                }

                let end = Instant::now();

                log::error!(
                    "Fetching a chunk of height {:?} took {:?}",
                    height,
                    end - start
                );

                let packet = LevelChunkWithLightS2CPlayPacket {
                    chunk_x,
                    chunk_z,
                    heightmaps: Nbt {
                        name: "".to_string(),
                        root: NbtCompound::new(),
                    },
                    data: ChunkSectionData { sections },
                    block_entities: vec![].into(),
                    sky_light_mask: vec![0].into(),
                    block_light_mask: vec![0].into(),
                    empty_sky_light_mask: vec![0].into(),
                    empty_block_light_mask: vec![0].into(),
                    sky_light_array: vec![].into(),
                    block_light_array: vec![].into(),
                };

                player
                    .write_packet(SetChunkCacheCenterS2CPlayPacket {
                        chunk_x: chunk_center.x().into(),
                        chunk_z: chunk_center.y().into(),
                    })
                    .await
                    .unwrap();
                player
                    .write_packet(ChunkBatchStartS2CPlayPacket {})
                    .await
                    .unwrap();
                player.write_packet(packet).await.unwrap();
                player
                    .write_packet(ChunkBatchFinishedS2CPlayPacket {
                        size: VarInt::from(1),
                    })
                    .await
                    .unwrap();
            });
        }
    }
}
