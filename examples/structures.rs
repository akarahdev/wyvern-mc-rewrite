use std::{sync::Arc, time::Duration};

use datafix::serialization::{Codec, DefaultCodec};
use wyvern_mc::{
    actors::ActorResult,
    blocks::{BlockState, Structure},
    components::DataComponentHolder,
    entities::{AttributeContainer, Attributes},
    events::{
        BreakBlockEvent, DimensionCreateEvent, PlaceBlockEvent, PlayerJoinEvent, ServerStartEvent,
    },
    player::PlayerComponents,
    runtime::Runtime,
    server::Server,
    values::{
        Gamemode, Vec3, id,
        nbt::{Nbt, NbtCompound, NbtOps},
    },
};

fn main() {
    env_logger::init();

    Server::builder()
        .event(on_server_start)
        .event(on_dim_init)
        .event(on_join)
        .event(on_break)
        .event(on_place)
        .registries(|registries| {
            registries.add_defaults();
        })
        .run();
}

fn on_server_start(event: Arc<ServerStartEvent>) -> ActorResult<()> {
    event.server.create_dimension(id!(example:root))?;

    Ok(())
}

fn on_dim_init(event: Arc<DimensionCreateEvent>) -> ActorResult<()> {
    let bytes = include_bytes!("./structure.nbt").to_vec();
    let nbt = Nbt::new(NbtCompound::try_from(bytes).unwrap());
    let structure = Structure::codec().decode(&NbtOps, &nbt).unwrap();

    structure.place(event.dimension.clone(), Vec3::new(0, 0, 0))?;
    Ok(())
}

fn on_join(event: Arc<PlayerJoinEvent>) -> ActorResult<()> {
    event.new_dimension.set(id![example:root]);
    event.player.set_gamemode(Gamemode::Survival)?;
    event.player.set(
        PlayerComponents::TELEPORT_POSITION,
        Vec3::new(0.0, 1.0, 0.0),
    )?;

    Runtime::spawn_task(move || {
        std::thread::sleep(Duration::from_millis(10000));
        event.player.set(
            PlayerComponents::ATTRIBUTES,
            AttributeContainer::new()
                .with(Attributes::MINING_EFFICIENCY, 100.0)
                .with(Attributes::MAX_HEALTH, 40.0),
        )?;
        Ok(())
    });
    Ok(())
}

fn on_break(event: Arc<BreakBlockEvent>) -> ActorResult<()> {
    let dim = event.player.dimension()?;
    dim.set_block(event.position, event.old_block.clone())?;
    Ok(())
}

fn on_place(event: Arc<PlaceBlockEvent>) -> ActorResult<()> {
    let dim = event.player.dimension()?;
    dim.set_block(event.position, BlockState::new(id![minecraft:air]))?;
    Ok(())
}
