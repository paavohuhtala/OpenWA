pub mod arrow;
pub mod base;
pub mod bit_grid;
pub mod cloud;
pub mod filter;
pub mod fire;
pub mod game_task;
pub mod mine_oil_drum;
pub mod missile;
mod overlays;
pub mod supply_crate;
pub mod team;
pub mod turn_game;
pub mod worm;

pub use arrow::{CTaskArrow, CTaskArrowVTable};
pub use base::{CTask, CTaskBfsIter, SharedDataIter, SharedDataNode, SharedDataTable, Task};
pub use cloud::{CTaskCloud, CTaskCloudVTable};
pub use filter::{CTaskFilter, CTaskFilterVTable};
pub use fire::{CTaskFire, CTaskFireVTable};
pub use game_task::{CGameTask, SoundEmitter, SoundEmitterVTable};
pub use mine_oil_drum::{CTaskMine, CTaskMineVTable, CTaskOilDrum, CTaskOilDrumVTable};
pub use missile::{CTaskMissile, CTaskMissileVTable, MissileType};
pub use overlays::{BungeeTrailTask, WeaponAimTask};
pub use supply_crate::{CTaskCrate, CTaskCrateVTable};
pub use team::{CTaskTeam, CTaskTeamVTable};
pub use turn_game::{CTaskTurnGame, CTaskTurnGameVTable, TurnGameCtx};
pub use worm::{CTaskWorm, CTaskWormVTable};

// Task trait impls — safe access to CTask base regardless of inheritance depth.
// CTask<V> impl is in base.rs (blanket impl).
unsafe impl<V: 'static> Task for CGameTask<V> {}
unsafe impl Task for CTaskTeam {}
unsafe impl Task for CTaskTurnGame {}
unsafe impl Task for CTaskFilter {}
unsafe impl Task for CTaskCloud {}
unsafe impl Task for CTaskFire {}
unsafe impl Task for CTaskWorm {}
unsafe impl Task for CTaskMissile {}
unsafe impl Task for CTaskArrow {}
unsafe impl Task for CTaskMine {}
unsafe impl Task for CTaskOilDrum {}
unsafe impl Task for CTaskCrate {}
