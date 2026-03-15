pub mod active_sound;
pub mod dssound;
pub mod music;
pub mod sound;
pub mod speech;
pub mod streaming;

pub use active_sound::{ActiveSoundEntry, ActiveSoundTable};
pub use dssound::DSSound;
pub use music::Music;
pub use sound::SoundId;
pub use speech::{SpeechLineId, SpeechLineTableEntry, SpeechSlotTable, SPEECH_LINE_TABLE_COUNT};
pub use streaming::StreamingAudio;
