/// Task/entity classification type.
///
/// Stored at offset 0x20 in BaseEntity. Used to identify the concrete type
/// of a task in the task hierarchy.
///
/// Source: wkJellyWorm Constants.h
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ClassType {
    None = 0,
    Task = 1,
    GameTask = 2,
    GameCollisionTask = 3,
    Control = 4,
    Game = 5,
    WorldRoot = 6,
    Filter = 7,
    Mine = 8,
    Canister = 9,
    Team = 10,
    Missile = 11,
    Arrow = 12,
    Animation = 13,
    Dirt = 14,
    Crate = 15,
    Flame = 16,
    AirStrike = 17,
    Worm = 18,
    OldWorm = 19,
    Drill = 20,
    Cross = 21,
    Smoke = 22,
    Cloud = 23,
    Fire = 24,
    Gas = 25,
    /// Girder placement task. Previously "FireBall" in RE sources — renamed
    /// after confirming the only creation site is FireWeapon__Girder (0x51E350).
    Girder = 26,
    SeaBubble = 27,
    Land = 28,
    ScoreBubble = 29,
    OilDrum = 30,
    Cpu = 31,
    SpriteAnimation = 32,
    CollisionManager = 33,
}

impl ClassType {
    pub const MAX: u32 = 34;
}

impl TryFrom<u32> for ClassType {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value < Self::MAX {
            // SAFETY: all values 0..34 are valid variants
            Ok(unsafe { core::mem::transmute(value) })
        } else {
            Err(value)
        }
    }
}
