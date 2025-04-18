use std::{fmt::Debug, pin::Pin, sync::Arc};

use crate::{
    actors::ActorResult, blocks::BlockState, dimension::Dimension, entities::Entity,
    item::ItemStack, player::Player, server::Server,
};

use wyvern_values::{DVec3, IVec2, IVec3, Id, Vec2, cell::Token};

macro_rules! event_bus {
    ($($name:ident : $t:ty)*) => {
        #[derive(Default)]
        pub struct EventBus {
            $(pub(crate) $name: Vec<Arc<dyn Fn(Arc<$t>) -> BoxedFuture + Sync + Send>>,)*
        }

        $(impl crate::events::Event for $t {
            fn add_handler<F: 'static + Future<Output = ActorResult<()>> + Send + Sync, N: 'static + Fn(Arc<$t>) -> F + Send + Sync>(bus: &mut EventBus, f: N) {
                bus.$name.push(Arc::new(move |event| {
                    let result = f(event);
                    Box::pin(result)
                }));
            }

            fn dispatch(self, bus: std::sync::Arc<EventBus>) {
                let tick1 = std::time::Instant::now();
                let tick2 = std::time::Instant::now();
                let time_time = tick2 - tick1;
                let start = std::time::Instant::now();
                let event = Arc::new(self);
                for event_func in bus.$name.clone().into_iter() {
                    $crate::runtime::Runtime::spawn_task(event_func(event.clone()));
                }
                let end = std::time::Instant::now();
                log::debug!("Event {:?} took {:?} to execute", std::any::type_name::<Self>(), (end - start) - (time_time));
            }
        })*

    };
}

event_bus! {
    on_join: PlayerJoinEvent
    on_dim_create: DimensionCreateEvent
    on_server_tick: ServerTickEvent
    on_player_move: PlayerMoveEvent
    on_chunk_load: ChunkLoadEvent
    on_command: PlayerCommandEvent
    on_server_start: ServerStartEvent
    on_place_block: PlaceBlockEvent
    start_break_block: StartBreakBlockEvent
    on_change_held_slot: ChangeHeldSlotEvent
    on_swap_hands: SwapHandsEvent
    on_drop_item: DropItemEvent
    on_block_break: BreakBlockEvent
    on_chat: ChatMessageEvent
    on_right_click: RightClickEvent
    on_attack_entity: PlayerAttackEntityEvent
    on_attack_player: PlayerAttackPlayerEvent
    on_left_click: PlayerLeftClickEvent
    on_load: PlayerLoadEvent
    on_respawn: PlayerRespawnEvent
}

impl Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EventBus { ... }")
    }
}

pub trait Event {
    fn add_handler<
        F: 'static + Future<Output = ActorResult<()>> + Send + Sync,
        N: 'static + Fn(Arc<Self>) -> F + Send + Sync,
    >(
        bus: &mut EventBus,
        f: N,
    );
    fn dispatch(self, bus: Arc<EventBus>);
}

pub type BoxedFuture = Pin<Box<dyn Future<Output = ActorResult<()>> + Sync + Send + 'static>>;

#[derive(Debug, Clone)]
pub struct DimensionCreateEvent {
    pub dimension: Dimension,
    pub server: Server,
}

#[derive(Debug, Clone)]
pub struct ChunkLoadEvent {
    pub dimension: Dimension,
    pub pos: IVec2,
}

#[derive(Debug, Clone)]
pub struct ServerTickEvent {
    pub server: Server,
}

#[derive(Debug, Clone)]
pub struct PlayerMoveEvent {
    pub player: Player,
    pub new_position: DVec3,
    pub new_direction: Vec2,
}

#[derive(Debug, Clone)]
pub struct PlayerCommandEvent {
    pub player: Player,
    pub command: String,
}

#[derive(Debug, Clone)]
pub struct StartBreakBlockEvent {
    pub player: Player,
    pub position: IVec3,
}

#[derive(Debug, Clone)]
pub struct StopBreakBlockEvent {
    pub player: Player,
    pub position: IVec3,
}

#[derive(Debug, Clone)]
pub struct BreakBlockEvent {
    pub player: Player,
    pub position: IVec3,
    pub old_block: BlockState,
}

#[derive(Debug, Clone)]
pub struct PlaceBlockEvent {
    pub player: Player,
    pub position: IVec3,
    pub block: BlockState,
}

#[derive(Debug)]
pub struct DropItemEvent {
    pub player: Player,
    pub item: ItemStack,
}

#[derive(Debug, Clone)]
pub struct RightClickEvent {
    pub player: Player,
}

#[derive(Debug, Clone)]
pub struct SwapHandsEvent {
    pub player: Player,
}

#[derive(Debug, Clone)]
pub struct ChangeHeldSlotEvent {
    pub player: Player,
    pub slot: i16,
}

#[derive(Debug, Clone)]
pub struct ServerStartEvent {
    pub server: Server,
}

#[derive(Debug, Clone)]
pub struct ChatMessageEvent {
    pub player: Player,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct PlayerJoinEvent {
    pub player: Player,
    pub new_dimension: Token<Id>,
}

#[derive(Debug, Clone)]
pub struct PlayerAttackEntityEvent {
    pub attacker: Player,
    pub victim: Entity,
}

#[derive(Debug, Clone)]
pub struct PlayerAttackPlayerEvent {
    pub attacker: Player,
    pub victim: Player,
}

#[derive(Debug, Clone)]
pub struct PlayerLeftClickEvent {
    pub player: Player,
}

#[derive(Debug, Clone)]
pub struct PlayerLoadEvent {
    pub player: Player,
}

#[derive(Debug, Clone)]
pub struct PlayerRespawnEvent {
    pub player: Player,
}
