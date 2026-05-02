//! EntityActivityQueue — most-recent-activity rank table embedded in
//! `GameWorld + 0x600`.
//!
//! Every `WorldEntity` subclass (Worm, OldWorm, Crate, Mine, OilDrum, Cross,
//! Canister, ...) acquires a slot here in its constructor and releases it in
//! its destructor. The queue assigns each slot a **rank** that gets reset
//! to "newest" each time the owning entity hits an activity edge — selecting
//! a weapon, finishing movement, ending a turn, etc.
//!
//! **Scope is broader than its single known consumer.** The only `ages[]`
//! reader we've traced is `WormEntity::BehaviorTick`'s water-death path,
//! which forwards the rank as a stagger delay to `ScoreBubbleEntity`'s ctor
//! so simultaneous drownings produce sequentially-popping bubbles. Whether
//! the rank drives anything else (or whether this is partly vestigial WA
//! design) is unclear; the name reflects the **shape** (recency-ordered slot
//! pool) rather than a specific purpose.
//!
//! **Layout**: three parallel `[u32; 0x400]` arrays plus a 3-DWORD trailer
//! (total 0x300C bytes):
//!
//! | Offset | Field        | Description                                    |
//! |-------:|--------------|------------------------------------------------|
//! | 0x0000 | `free_pool`  | LIFO of unused slot IDs                        |
//! | 0x1000 | `active_ids` | Compact list of currently-held IDs (len=count) |
//! | 0x2000 | `ages`       | Per-slot rank, indexed by ID                   |
//! | 0x3000 | `pool_head`  | Free-pool top                                  |
//! | 0x3004 | `count`      | Number of active slots                         |
//! | 0x3008 | `capacity`   | Configured capacity (≤ 0x100 in practice)      |
//!
//! Acquire pops `free_pool[--pool_head]`, sets `ages[id] = count`, appends
//! to `active_ids[count++]`. The `ResetRank` op zeroes that slot's age and
//! "ages up" every younger slot, keeping a contiguous rank.

use crate::FieldRegistry;

/// Most-recent-activity rank table for `WorldEntity` subclasses. Lives at
/// `GameWorld + 0x600`, total size `0x300C` bytes. See module docs.
///
/// The arrays are sized for the maximum capacity (`0x400`); only
/// `[0..capacity]` are live (`game_version >= 0x3C` -> `0x400`,
/// older versions -> `0x100`).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct EntityActivityQueue {
    /// 0x0000: Free-pool LIFO of unused slot IDs. `init` fills with the
    /// identity permutation `[0..capacity]`. Acquire reads
    /// `free_pool[pool_head - 1]` then decrements `pool_head`.
    pub free_pool: [u32; 0x400],
    /// 0x1000: Compact list of currently-acquired slot IDs. Valid range is
    /// `[0..count]`. Acquire appends a new ID at `[count]`.
    pub active_ids: [u32; 0x400],
    /// 0x2000: Per-slot rank, indexed by slot ID. `0xFFFFFFFF` = unused;
    /// otherwise a non-negative value where smaller = more recent. Set to
    /// `count` (oldest-current rank) at acquisition; `ResetRank` zeroes
    /// `ages[id]` and increments every entry whose age was less than the
    /// reset slot's age — keeping the rank ordering contiguous.
    pub ages: [u32; 0x400],
    /// 0x3000: Free-pool top. Acquire decrements; release-all
    /// (`ResetRank(slot < 0)`) doesn't touch this directly but sets
    /// `count = 0`.
    pub pool_head: u32,
    /// 0x3004: Number of currently-active slots.
    pub count: u32,
    /// 0x3008: Configured capacity. Set by `init`; the `ResetRank`
    /// release-all path gates on `capacity <= 0x100`.
    pub capacity: u32,
}

const _: () = assert!(core::mem::size_of::<EntityActivityQueue>() == 0x300C);

impl EntityActivityQueue {
    /// Pure Rust port of `EntityActivityQueue::Init` (0x00541620, was
    /// misnamed `SpriteGfxTable__Init`). Initialises the free pool to the
    /// identity permutation and marks all slots as un-acquired.
    ///
    /// Convention at the WA call site: `__fastcall(ECX = this, EDX = capacity)`,
    /// plain RET.
    pub unsafe fn init(this: *mut EntityActivityQueue, capacity: u32) {
        unsafe {
            for i in 0..capacity {
                (*this).free_pool[i as usize] = i;
                (*this).ages[i as usize] = 0xFFFFFFFF;
            }
            (*this).pool_head = capacity;
            (*this).count = 0;
            (*this).capacity = capacity;
        }
    }
}

crate::define_addresses! {
    class "EntityActivityQueue" {
        /// `EntityActivityQueue::Init` (0x00541620) — fastcall(ECX=this,
        /// EDX=capacity). Was previously misnamed `SpriteGfxTable__Init`.
        fn/Fastcall ENTITY_ACTIVITY_QUEUE_INIT = 0x00541620;
    }
}
