pub mod arrow;
pub mod base;
pub mod cloud;
pub mod filter;
pub mod fire;
pub mod game_entity;
pub mod mine;
pub mod missile;
pub mod oil_drum;
mod overlays;
pub mod supply_crate;
pub mod team;
pub mod team_message;
pub mod world_root;
pub mod worm;
pub mod worm_handle_message;

pub use arrow::{ArrowEntity, ArrowEntityVtable};
pub use base::{
    BaseEntity, BaseEntityBfsIter, BaseEntityVtable, Entity, SharedDataIter, SharedDataNode,
    SharedDataTable, Vtable,
};
pub use cloud::{CloudEntity, CloudEntityVtable, CloudType};
pub use filter::{FilterEntity, FilterEntityVtable};
pub use fire::{FireEntity, FireEntityVtable};
pub use game_entity::{SoundEmitter, SoundEmitterVtable, SubclassData, WorldEntity};
pub use mine::{MineEntity, MineEntityVtable};
pub use missile::{MissileEntity, MissileEntityVtable, MissileSubclassData, MissileType};
pub use oil_drum::{OilDrumEntity, OilDrumEntityVtable};
pub use overlays::WeaponAimEntity;
pub use supply_crate::{CrateEntity, CrateEntityVtable};
pub use team::{TeamEntity, TeamEntityVtable};
pub use team_message::TeamMessage;
pub use world_root::{MatchCtx, WorldRootEntity, WorldRootEntityVtable};
pub use worm::{KnownWormState, WormEntity, WormEntityVtable, WormSubclassData};

// Entity trait impls — safe access to BaseEntity base regardless of inheritance depth.
// BaseEntity<V> impl is in base.rs (blanket impl).
unsafe impl<V: Vtable, S: SubclassData> Entity for WorldEntity<V, S> {}
unsafe impl Entity for TeamEntity {}
unsafe impl Entity for WorldRootEntity {}
unsafe impl Entity for FilterEntity {}
unsafe impl Entity for CloudEntity {}
unsafe impl Entity for FireEntity {}
unsafe impl Entity for WormEntity {}
unsafe impl Entity for MissileEntity {}
unsafe impl Entity for ArrowEntity {}
unsafe impl Entity for MineEntity {}
unsafe impl Entity for OilDrumEntity {}
unsafe impl Entity for CrateEntity {}
