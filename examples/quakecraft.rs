use std::sync::Arc;

use datafix::serialization::{Codec, DefaultCodec};
use wyvern_mc::{
    actors::ActorResult,
    blocks::{BlockState, Structure},
    components::DataComponentHolder,
    datatypes::{
        nbt::{Nbt, NbtCompound, NbtOps},
        particle::Particle,
        regval::DimensionType,
        text::{Text, TextColor},
    },
    entities::{AttributeContainer, Attributes},
    events::{
        BreakBlockEvent, ChatMessageEvent, DimensionCreateEvent, PlaceBlockEvent, PlayerJoinEvent,
        PlayerLeftClickEvent, PlayerLoadEvent, RightClickEvent, ServerStartEvent, ServerTickEvent,
    },
    inventory::Inventory,
    item::{ItemComponents, ItemStack},
    macros::server,
    player::{Player, PlayerComponents},
    server::{Server, ServerBuilder, registries::RegistryKeys},
    values::{DVec3, IVec3, Vec2Ext, id},
};

#[server]
fn server() -> ServerBuilder {
    Server::builder()
        .task_threads(1)
        .event(on_server_start)
        .event(on_dim_init)
        .event(on_join)
        .event(on_break)
        .event(on_place)
        .event(on_shoot)
        .event(on_chat)
        .event(on_tick)
        .event(on_dash)
        .event(on_load)
        .registries(|registries| {
            registries.get_mut(RegistryKeys::DIMENSION_TYPE).insert(
                id![minecraft:overworld],
                DimensionType::default().height(1024).min_y(0),
            );
        })
}

async fn on_server_start(event: Arc<ServerStartEvent>) -> ActorResult<()> {
    event
        .server
        .create_dimension(id!(example:root), id![minecraft:overworld])?;
    event.server.set_default_dimension(id![example:root])?;

    Ok(())
}

async fn on_dim_init(event: Arc<DimensionCreateEvent>) -> ActorResult<()> {
    let bytes = include_bytes!("./assets/quake.nbt").to_vec();
    let nbt = Nbt::new(NbtCompound::try_from(bytes).unwrap());
    let structure = Structure::codec().decode_start(&NbtOps, &nbt).unwrap();

    structure.place(event.dimension.clone(), IVec3::new(0, 0, 0))?;

    println!("Placed structure!");
    Ok(())
}

async fn on_join(event: Arc<PlayerJoinEvent>) -> ActorResult<()> {
    event.new_dimension.set(id![example:root]);
    event.player.inventory()?.set_slot(
        36,
        ItemStack::new(id![minecraft:iron_hoe])
            .with(ItemComponents::ITEM_NAME, Text::literal("Railgun")),
    )?;
    event.player.set(
        PlayerComponents::ATTRIBUTES,
        AttributeContainer::new().with(Attributes::ATTACK_SPEED, 1000.0),
    )?;
    respawn_player(&event.player)?;
    Ok(())
}

async fn on_load(event: Arc<PlayerLoadEvent>) -> ActorResult<()> {
    event.player.send_message(Text::literal("a"))?;
    respawn_player(&event.player)?;
    Ok(())
}

async fn on_break(event: Arc<BreakBlockEvent>) -> ActorResult<()> {
    let dim = event.player.dimension()?;
    dim.set_block(event.position, event.old_block.clone())?;
    Ok(())
}

async fn on_place(event: Arc<PlaceBlockEvent>) -> ActorResult<()> {
    let dim = event.player.dimension()?;
    dim.set_block(event.position, BlockState::new(id![minecraft:air]))?;
    Ok(())
}

async fn on_chat(event: Arc<ChatMessageEvent>) -> ActorResult<()> {
    for player in Server::get()?.players()? {
        player.send_message(Text::literal(format!(
            "<{}> {}",
            event.player.get(PlayerComponents::USERNAME)?,
            event.message
        )))?;
    }
    Ok(())
}

async fn on_tick(_event: Arc<ServerTickEvent>) -> ActorResult<()> {
    for player in Server::get()?.players()? {
        player.set(
            PlayerComponents::SIDEBAR_NAME,
            Text::literal("QUAKECRAFT")
                .with_color(TextColor::new(255, 255, 0))
                .bold(true),
        )?;
        player.set(
            PlayerComponents::SIDEBAR_LINES,
            vec![
                Text::literal(""),
                Text::literal("Kills: ")
                    .with_color(TextColor::new(133, 133, 133))
                    .and_then(Text::literal("Untracked").with_color(TextColor::new(255, 133, 133))),
                Text::literal(""),
                Text::literal("www.example.org").with_color(TextColor::new(255, 255, 0)),
            ],
        )?;
        player.set(PlayerComponents::SIDEBAR_PRESENT, true)?;
    }
    Ok(())
}

async fn on_shoot(event: Arc<RightClickEvent>) -> ActorResult<()> {
    let position = event.player.get(PlayerComponents::POSITION)?;
    let direction = event
        .player
        .get(PlayerComponents::DIRECTION)?
        .to_3d_direction()
        .map(|x| x / 2.0);
    let mut step = position.with_y(position[1] + 1.8);
    let players = event.player.dimension()?.players()?;
    for _ in 1..120 {
        step = step
            .with_x(step[0] + direction[0])
            .with_y(step[1] + direction[1])
            .with_z(step[2] + direction[2]);

        for subplayer in &players {
            let subplayer = Server::get()?.player(*subplayer)?;
            subplayer.play_particle(step, Particle::new(id![minecraft:electric_spark]))?;
        }

        if *event
            .player
            .dimension()?
            .get_block(step.floor().as_ivec3())?
            .name()
            != id![minecraft:air]
        {
            break;
        }

        for player in &players {
            let player = Server::get()?.player(*player)?;
            let position = player.get(PlayerComponents::POSITION)?;
            if player.get(PlayerComponents::USERNAME)
                == event.player.get(PlayerComponents::USERNAME)
            {
                continue;
            }

            let dx = (step[0] - position[0]).abs();
            let dy = (step[1] - (position[1] + 1.0)).abs();
            let dz = (step[2] - position[2]).abs();

            if dx <= 0.5 && dz <= 0.5 && dy <= 1.0 {
                respawn_player(&player)?;

                for subplayer in &players {
                    let subplayer = Server::get()?.player(*subplayer)?;
                    subplayer.send_message(Text::literal(format!(
                        "{} pommed {}",
                        event.player.get(PlayerComponents::USERNAME)?,
                        player.get(PlayerComponents::USERNAME)?
                    )))?;
                }

                return Ok(());
            }
        }
    }
    Ok(())
}

fn respawn_player(player: &Player) -> ActorResult<()> {
    player.send_message(Text::literal("a"))?;

    let spawn_pos = loop {
        let rand_x = rand::random_range(0.0..20.0);
        let rand_y = rand::random_range(0.0..20.0);
        let rand_z = rand::random_range(0.0..20.0);

        let mut pos = DVec3::new(rand_x, rand_y, rand_z);
        let mut descent_steps = 0;

        while *player
            .dimension()?
            .get_block(pos.floor().as_ivec3())?
            .name()
            == id![minecraft:air]
        {
            pos = pos.with_y(pos[1] - 0.5);
            descent_steps += 1;
            if descent_steps > 100 {
                break;
            }
        }

        // println!("pos: {:?} ({:#?})", pos, descent_steps);

        if descent_steps > 10 {
            continue;
        }

        let candidate_spawn = pos.with_y(pos[1] + 1.0);

        if *player
            .dimension()?
            .get_block(candidate_spawn.floor().as_ivec3())?
            .name()
            != id![minecraft:air]
        {
            continue;
        }

        let head_space = candidate_spawn.with_y(candidate_spawn[1] + 1.0);
        if *player
            .dimension()?
            .get_block(head_space.floor().as_ivec3())?
            .name()
            != id![minecraft:air]
        {
            continue;
        }

        break candidate_spawn;
    };

    player.set(PlayerComponents::TELEPORT_POSITION, spawn_pos)?;
    Ok(())
}

async fn on_dash(event: Arc<PlayerLeftClickEvent>) -> ActorResult<()> {
    let dir = event
        .player
        .get(PlayerComponents::DIRECTION)?
        .to_3d_direction()
        .with_y(0.1);
    event.player.set(PlayerComponents::TELEPORT_VELOCITY, dir)?;
    Ok(())
}
