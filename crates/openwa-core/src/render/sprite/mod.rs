pub mod frame_cache;
pub mod lzss;
pub mod spr;
pub mod sprite;
pub mod sprite_id;
pub mod sprite_op;

pub use frame_cache::frame_cache_allocate;
pub use lzss::sprite_lzss_decode;
pub use spr::{ParsedSprite, SprError, SprHeader};
pub use sprite::{
    KnownSpriteId, Sprite, SpriteBank, SpriteBankBboxEntry, SpriteBankFrame,
    SpriteBankSubframeCache, SpriteBankVtable, SpriteFlags, SpriteFrame, SpriteOp,
    SpriteSubframeCache, SpriteVtable,
};
