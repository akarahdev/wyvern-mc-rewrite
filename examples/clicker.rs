use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

use voxidian_protocol::packet::s2c::play::ScreenWindowKind;
use wyvern_mc::{
    actors::ActorResult,
    dimension::blocks::BlockState,
    events::{
        DimensionCreateEvent, PlayerJoinEvent, RightClickEvent, ServerStartEvent, ServerTickEvent,
        SwapHandsEvent,
    },
    inventory::{Inventory, ItemStack},
    key,
    runtime::Runtime,
    server::Server,
    values::{
        Key, SoundCategory, Sounds, Texts, Uuid, Vec3,
        regval::{DimensionType, PaintingVariant, WolfVariant},
    },
};

static COUNTER: LazyLock<Mutex<HashMap<Uuid, i32>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[tokio::main]
async fn main() {
    env_logger::init();

    Runtime::tokio();

    Server::builder()
        .event(on_server_start)
        .event(on_dim_init)
        .event(on_join)
        .event(on_tick)
        .event(on_right_click)
        .event(on_swap_hands)
        .registries(|registries| {
            registries.wolf_variant(Key::new("minecraft", "pale"), WolfVariant {
                angry_texture: Key::empty(),
                wild_texture: Key::empty(),
                tame_texture: Key::empty(),
                biomes: Vec::new(),
            });
            registries.painting_variant(Key::new("minecraft", "something_idk"), PaintingVariant {
                asset: Key::empty(),
                width: 1,
                height: 1,
            });
            registries.dimension_type(Key::new("minecraft", "overworld"), DimensionType::default());
        })
        .run()
        .await;
}

async fn on_server_start(event: Arc<ServerStartEvent>) -> ActorResult<()> {
    event.server.create_dimension(key!(clicker:root)).await?;

    Ok(())
}

async fn on_dim_init(event: Arc<DimensionCreateEvent>) -> ActorResult<()> {
    for x in 0..6 {
        for z in 0..6 {
            event
                .dimension
                .set_block(
                    Vec3::new(x, 0, z),
                    BlockState::new(key![minecraft:grass_block]),
                )
                .await?;
        }
    }
    Ok(())
}

async fn on_join(event: Arc<PlayerJoinEvent>) -> ActorResult<()> {
    event.new_dimension.set(key!(clicker:root));

    event
        .player
        .inventory()?
        .set_slot(40, ItemStack::new(Key::new("minecraft", "diamond")))
        .await?;
    Ok(())
}

async fn on_tick(event: Arc<ServerTickEvent>) -> ActorResult<()> {
    for player in event.server.players().await? {
        let uuid = player.uuid().await?;

        let count = {
            let mut counter = COUNTER.lock().unwrap();

            match counter.get(&uuid).copied() {
                None => {
                    counter.insert(uuid, 0);
                    0
                }
                Some(count) => count,
            }
        };

        player
            .send_action_bar(Texts::literal(format!("Clicks: {:?}", count)))
            .await?;
    }
    Ok(())
}

async fn on_right_click(event: Arc<RightClickEvent>) -> ActorResult<()> {
    let uuid = event.player.uuid().await?;
    {
        let mut counter = COUNTER.lock().unwrap();
        if let Some(number) = counter.get_mut(&uuid) {
            *number += 1;
        };
    }

    event
        .player
        .play_sound(
            Sounds::BLOCK_AMETHYST_CLUSTER_BREAK
                .pitch(1.5)
                .volume(0.7)
                .category(SoundCategory::Master),
        )
        .await?;

    Ok(())
}

async fn on_swap_hands(event: Arc<SwapHandsEvent>) -> ActorResult<()> {
    event
        .player
        .open_screen(ScreenWindowKind::Generic9x1)
        .await?;
    Ok(())
}
