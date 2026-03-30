pub mod active_sound;
pub mod dssound;
pub mod music;
pub mod sound;
pub mod sound_ops;
pub mod speech;
pub mod streaming;

pub use active_sound::{ActiveSoundEntry, ActiveSoundTable};
pub use dssound::{
    destructor as dssound_destructor, is_channel_finished, is_slot_loaded, load_wav,
    noop as dssound_noop, play_sound, play_sound_pooled, release_finished,
    returns_0 as dssound_returns_0, returns_1 as dssound_returns_1, set_channel_volume,
    set_master_volume, set_pan, set_volume_params, stop_channel,
    sub_destructor as dssound_sub_destructor, update_channels,
};
pub use dssound::{ChannelDescriptor, DSSound, DSSoundVtable};
pub use music::Music;
pub use sound::{KnownSoundId, SoundId};
pub use speech::{SpeechLineId, SpeechLineTableEntry, SpeechSlotTable, SPEECH_LINE_TABLE_COUNT};
pub use streaming::StreamingAudio;
