pub mod active_sound;
pub mod dssound;
pub mod music;
pub mod sound;
pub mod speech;
pub mod streaming;

pub use active_sound::{ActiveSoundEntry, ActiveSoundTable};
pub use dssound::{DSSound, DSSoundVtable, ChannelDescriptor};
pub use dssound::{is_slot_loaded, load_wav, noop as dssound_noop, returns_0 as dssound_returns_0, returns_1 as dssound_returns_1};
pub use music::Music;
pub use sound::SoundId;
pub use speech::{SpeechLineId, SpeechLineTableEntry, SpeechSlotTable, SPEECH_LINE_TABLE_COUNT};
pub use streaming::StreamingAudio;
