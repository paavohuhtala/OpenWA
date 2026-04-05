pub mod active_sound;
pub mod dssound;
pub mod known_sound_id;
pub mod music;
pub mod sound_id;
pub mod sound_ops;
pub mod sound_queue;
pub mod speech;
pub mod speech_ops;
pub mod wav_player;

pub use active_sound::{ActiveSoundEntry, ActiveSoundTable};
pub use dssound::{
    destructor as dssound_destructor, is_channel_finished, is_slot_loaded, load_wav,
    noop as dssound_noop, play_sound, play_sound_pooled, release_finished,
    returns_0 as dssound_returns_0, returns_1 as dssound_returns_1, set_channel_volume,
    set_master_volume, set_pan, set_volume_params, stop_channel,
    sub_destructor as dssound_sub_destructor, update_channels,
};
pub use dssound::{ChannelDescriptor, DSSound, DSSoundVtable};
pub use known_sound_id::KnownSoundId;
pub use music::{Music, MusicVtable, StreamingAudio};
pub use sound_id::SoundId;
pub use sound_queue::SoundQueueEntry;
pub use speech::{SpeechLineId, SpeechLineTableEntry, SpeechSlotTable, SPEECH_LINE_TABLE_COUNT};
