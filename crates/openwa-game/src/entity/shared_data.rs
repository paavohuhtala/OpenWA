//! BaseEntity shared-data hash table.
//!
//! Every entity in a single game tree inherits the same `shared_data` pointer
//! (allocated by the GameRuntime root). The first 0x400 bytes of that block
//! are 256 × 4 bucket heads of a chained hash table keyed by a `(key_esi,
//! key_edi)` pair. Constructors of named entity types insert themselves; other
//! code looks them up to find the per-game singletons (WorldRoot, Land, the
//! filters, …) and the per-team / per-worm slots.
//!
//! Both names trace back to the registers MSVC chose for the original
//! `SharedData__Insert` (`__usercall(stack, stack; ESI, EDI)`).
//!
//! Insert sites (Ghidra VAs):
//! - `SharedData__Insert` (0x005406A0) — explicit `__usercall` callees:
//!     - `WormEntity::Constructor` (0x0050BFB0) writes `(team*16 + worm, 2)`.
//!     - `FilterEntity::Constructor` (0x0054F3D0) writes `(0, kind)` where
//!       `kind` is the ECX argument from the spawning `TeamEntity` helper.
//!     - `BaseEntity::Constructor_Maybe` (0x00562640) — replay/savegame
//!       deserialiser; reads both keys from the input stream.
//! - Inlined hash-bucket writes (no call to 0x005406A0):
//!     - `LandEntity::Constructor` (0x00505440) writes `(0, 1)`.
//!     - `GameRuntime::Constructor_0` (0x00550E70) writes `(0, 0x14)` after
//!       finishing all subordinate constructions — this is what publishes
//!       the `WorldRootEntity` to the rest of the tree.
//!     - `TeamEntity::Constructor` (0x00555BB0) writes `(team_idx, 0x15)`.
//!     - `SpriteAnimEntity::Constructor` (0x005466C0) writes `(0, 0x1A)`.
//!
//! Removed by `SharedData__Remove` (0x00540700), called only from
//! `BaseEntity::Destructor_Maybe` (0x0056287A).

use crate::entity::Entity;

use super::base::BaseEntity;
use super::filter::FilterEntity;
use super::team::TeamEntity;
use super::world_root::WorldRootEntity;
use super::worm::WormEntity;

/// Known `key_edi` values for the SharedData hash table.
///
/// The `key_esi` is `0` for singletons, `team_idx` for [`TEAM_ENTITY`], and
/// `team_idx * 16 + worm_idx` for [`WORM_ENTITY`].
pub mod key {
    /// `LandEntity` — the per-game terrain singleton. `key_esi` = 0.
    /// Inserted by `LandEntity::Constructor` (0x00505440) via direct bucket write.
    pub const LAND_ENTITY: u32 = 0x01;

    /// `WormEntity`. `key_esi` = `team_idx * 16 + worm_idx` (both 1-based).
    /// Inserted by `WormEntity::Constructor` (0x0050BFB0) via `SharedData__Insert`.
    pub const WORM_ENTITY: u32 = 0x02;

    /// `WorldRootEntity` (root of the in-game tree). `key_esi` = 0.
    /// Inserted by `GameRuntime::Constructor_0` (0x00550E70) via direct bucket
    /// write at the end of the ctor — this is the publication point that lets
    /// any other entity look the root up by its `shared_data` pointer.
    pub const WORLD_ROOT_ENTITY: u32 = 0x14;

    /// `TeamEntity`. `key_esi` = `team_idx` (1-based).
    /// Inserted by `TeamEntity::Constructor` (0x00555BB0) via direct bucket write.
    pub const TEAM_ENTITY: u32 = 0x15;

    /// Water `FilterEntity`. `key_esi` = 0.
    /// Inserted by `TeamEntity::SpawnFilterEntity_Water` (0x005520D0) →
    /// `FilterEntity::Constructor` with ECX = 0x17.
    pub const FILTER_WATER: u32 = 0x17;

    /// Cloud `FilterEntity`. `key_esi` = 0.
    /// Inserted by `TeamEntity::SpawnFilterEntity_Clouds` (0x00552040) →
    /// `FilterEntity::Constructor` with ECX = 0x18. Parents the bubble pool.
    pub const FILTER_CLOUDS: u32 = 0x18;

    /// Physics `FilterEntity`. `key_esi` = 0.
    /// Inserted by `TeamEntity::SpawnFilterEntity_Physics` (0x00552190) →
    /// `FilterEntity::Constructor` with ECX = 0x19. Parents mines, oil drums,
    /// loose projectiles — anything affected by the physics tick.
    pub const FILTER_PHYSICS: u32 = 0x19;

    /// `SpriteAnimEntity` — per-game sprite-animation pool singleton.
    /// `key_esi` = 0. Inserted by `SpriteAnimEntity::Constructor`
    /// (0x005466C0) via direct bucket write.
    pub const SPRITE_ANIM_ENTITY: u32 = 0x1A;

    /// Weather `FilterEntity`. `key_esi` = 0. Only created when the scheme
    /// has weather enabled. Inserted by `TeamEntity::CreateWeatherFilter`
    /// (0x00552960) → `FilterEntity::Constructor` with ECX = 0x1B.
    pub const FILTER_WEATHER: u32 = 0x1B;
}

/// A 0x30-byte node in BaseEntity's shared-data entity hash table.
///
/// Allocated by the inserter (typically `SharedData__Insert` at 0x005406A0,
/// occasionally inlined into the entity constructor — see the module-level
/// docs for the full producer list). All game entity types share the same
/// 256-bucket table at `BaseEntity.shared_data`. The vtable pointer at
/// `entity[0]` identifies the object type once a node has been resolved.
///
/// Hash function (from `SharedData__Insert`):
/// ```text
/// bucket = (key_esi * 0x11 + key_edi) & 0x800000ff;
/// if (int)bucket < 0 { bucket = bucket.wrapping_sub(1) | 0xffffff00; bucket += 1; }
/// ```
/// In practice (small positive key values), this reduces to:
/// `bucket = (key_esi * 0x11 + key_edi) & 0xff`.
#[repr(C)]
pub struct SharedDataNode {
    /// +0x00: Next node in this bucket's linked list (null = end).
    pub next: *mut SharedDataNode,
    /// +0x04: EDI register value at registration time.
    pub key_edi: u32,
    /// +0x08: ESI register value at registration time.
    pub key_esi: u32,
    /// +0x0C: Registered entity pointer (first DWORD = vtable).
    pub entity: *mut u8,
    /// +0x10..0x2F: Unused allocation padding.
    pub _padding: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<SharedDataNode>() == 0x30);

/// View of the 256-bucket entity hash table at `BaseEntity.shared_data`.
///
/// Root entities own 0x420 bytes of shared data:
/// - `0x000..0x3FF`: 256 × `*mut SharedDataNode` bucket heads
/// - `0x400..0x41F`: Other root-entity data (layout unknown)
///
/// All entities in the same game tree inherit the same `shared_data` pointer,
/// so any entity can be used to access the full table. Use [`Self::iter`] to
/// walk all registered entities and filter by vtable address; use the typed
/// getters (e.g. [`Self::world_root`], [`Self::team`]) for the well-known
/// singletons documented in the [`key`] module.
pub struct SharedDataTable {
    buckets: *const *mut SharedDataNode,
}

impl SharedDataTable {
    /// Construct from a raw `BaseEntity.shared_data` pointer.
    ///
    /// # Safety
    /// `ptr` must point to a valid shared-data region of at least 256 × 4 = 1024 bytes.
    unsafe fn from_ptr(ptr: *mut u8) -> Self {
        Self {
            buckets: ptr as *const *mut SharedDataNode,
        }
    }

    /// Construct from a `BaseEntity` pointer (reads `entity.shared_data`).
    ///
    /// # Safety
    /// `entity` must be a valid, aligned `BaseEntity` pointer.
    pub unsafe fn from_entity(entity: *const impl Entity) -> Self {
        unsafe {
            let base = (*entity).entity();
            Self::from_ptr(base.shared_data)
        }
    }

    /// Compute the bucket index for a (key_esi, key_edi) pair.
    ///
    /// Exact transcription of the hash in `SharedData__Insert`.
    fn bucket_for(key_esi: u32, key_edi: u32) -> u32 {
        let mut h = key_esi.wrapping_mul(0x11).wrapping_add(key_edi) & 0x800000ff;
        if (h as i32) < 0 {
            h = h.wrapping_sub(1) | 0xffffff00;
            h = h.wrapping_add(1);
        }
        h
    }

    /// Look up an entity by raw key pair. Returns the entity pointer, or null.
    ///
    /// Pure Rust equivalent of `SharedData__Lookup` (0x004FDF90),
    /// `__fastcall(ECX=key_esi, EDX=key_edi, stack=entity)` in the original.
    ///
    /// Private — callers must go through one of the typed getters
    /// ([`Self::world_root`], [`Self::team`], …) so the documented `(esi,
    /// edi) → entity type` mapping in the [`key`] module stays the only
    /// place those magic numbers appear.
    ///
    /// # Safety
    /// The table and all linked nodes must be valid.
    unsafe fn lookup(&self, key_esi: u32, key_edi: u32) -> *mut u8 {
        unsafe {
            let bucket = Self::bucket_for(key_esi, key_edi) as usize;
            let mut node = *self.buckets.add(bucket);
            while !node.is_null() {
                if (*node).key_edi == key_edi && (*node).key_esi == key_esi {
                    return (*node).entity;
                }
                node = (*node).next;
            }
            core::ptr::null_mut()
        }
    }

    /// Typed lookup helper used by the per-key getters below — returns `None`
    /// if the slot is empty, otherwise a non-null `*mut T`. Private for the
    /// same reason as [`Self::lookup`].
    ///
    /// # Safety
    /// The table must be valid, and the registered entity at `(key_esi,
    /// key_edi)` must actually have type `T` per the [`key`] mapping.
    unsafe fn lookup_as<T>(&self, key_esi: u32, key_edi: u32) -> Option<*mut T> {
        let ptr = unsafe { self.lookup(key_esi, key_edi) } as *mut T;
        if ptr.is_null() { None } else { Some(ptr) }
    }

    /// Iterate all nodes across all 256 buckets.
    ///
    /// # Safety
    /// The table and all linked nodes must be valid and not concurrently modified.
    #[allow(dead_code)]
    unsafe fn iter(&self) -> SharedDataIter {
        SharedDataIter {
            buckets: self.buckets,
            bucket: 0,
            node: core::ptr::null_mut(),
        }
    }

    // -----------------------------------------------------------------------
    // Typed getters for the known [`key`] entries. `None` means the slot is
    // empty (e.g. before the producer ctor has run, or after teardown — or,
    // for [`Self::filter_weather`], when the scheme disables weather).
    // -----------------------------------------------------------------------

    /// `WorldRootEntity` at `(0, 0x14)`.
    ///
    /// # Safety
    /// See [`Self::lookup_as`].
    pub unsafe fn world_root(&self) -> Option<*mut WorldRootEntity> {
        unsafe { self.lookup_as(0, key::WORLD_ROOT_ENTITY) }
    }

    /// `TeamEntity` at `(team_idx, 0x15)`. `team_idx` is 1-based.
    ///
    /// # Safety
    /// See [`Self::lookup_as`].
    pub unsafe fn team(&self, team_idx: u32) -> Option<*mut TeamEntity> {
        unsafe { self.lookup_as(team_idx, key::TEAM_ENTITY) }
    }

    /// `WormEntity` at `(team_idx * 16 + worm_idx, 2)`. Both indices 1-based.
    ///
    /// # Safety
    /// See [`Self::lookup_as`].
    pub unsafe fn worm(&self, team_idx: u32, worm_idx: u32) -> Option<*mut WormEntity> {
        unsafe { self.lookup_as(team_idx * 16 + worm_idx, key::WORM_ENTITY) }
    }

    /// Water `FilterEntity` at `(0, 0x17)`.
    ///
    /// # Safety
    /// See [`Self::lookup_as`].
    pub unsafe fn filter_water(&self) -> Option<*mut FilterEntity> {
        unsafe { self.lookup_as(0, key::FILTER_WATER) }
    }

    /// Cloud `FilterEntity` at `(0, 0x18)`.
    ///
    /// # Safety
    /// See [`Self::lookup_as`].
    pub unsafe fn filter_clouds(&self) -> Option<*mut FilterEntity> {
        unsafe { self.lookup_as(0, key::FILTER_CLOUDS) }
    }

    /// Physics `FilterEntity` at `(0, 0x19)`.
    ///
    /// # Safety
    /// See [`Self::lookup_as`].
    pub unsafe fn filter_physics(&self) -> Option<*mut FilterEntity> {
        unsafe { self.lookup_as(0, key::FILTER_PHYSICS) }
    }

    /// Weather `FilterEntity` at `(0, 0x1B)`. Only present when the scheme
    /// enables weather (`world.game_info[+0x777C] == 0`).
    ///
    /// # Safety
    /// See [`Self::lookup_as`].
    pub unsafe fn filter_weather(&self) -> Option<*mut FilterEntity> {
        unsafe { self.lookup_as(0, key::FILTER_WEATHER) }
    }

    /// `LandEntity` at `(0, 1)`. No Rust struct yet — returned as `BaseEntity`.
    ///
    /// # Safety
    /// See [`Self::lookup_as`].
    pub unsafe fn land(&self) -> Option<*mut BaseEntity> {
        unsafe { self.lookup_as(0, key::LAND_ENTITY) }
    }

    /// `SpriteAnimEntity` at `(0, 0x1A)`. No Rust struct yet — returned as
    /// `BaseEntity`. This entity is the receiver for `EntityMessage 0x56`
    /// sprite-animation spawns; see `weapon_release::spawn_effect`.
    ///
    /// # Safety
    /// See [`Self::lookup_as`].
    pub unsafe fn sprite_anim_entity(&self) -> Option<*mut BaseEntity> {
        unsafe { self.lookup_as(0, key::SPRITE_ANIM_ENTITY) }
    }
}

/// Iterator over all [`SharedDataNode`]s in a [`SharedDataTable`].
///
/// Created by [`SharedDataTable::iter`]. Walks all 256 buckets in order,
/// following `next` pointers within each bucket.
pub struct SharedDataIter {
    buckets: *const *mut SharedDataNode,
    bucket: usize,
    node: *mut SharedDataNode,
}

impl Iterator for SharedDataIter {
    type Item = *mut SharedDataNode;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: caller of SharedDataTable::iter() guarantees table validity.
        unsafe {
            loop {
                if !self.node.is_null() {
                    let current = self.node;
                    self.node = (*self.node).next;
                    return Some(current);
                }
                if self.bucket >= 256 {
                    return None;
                }
                self.node = *self.buckets.add(self.bucket);
                self.bucket += 1;
            }
        }
    }
}
