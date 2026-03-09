/// Speech line IDs for worm voice lines.
///
/// Source: speech line table at WA.exe 0x6AF770 (.rdata).
/// 61 entries mapping `{id, filename_ptr}` pairs. 54 unique IDs (1-56,
/// gaps at 21 and 54). Some IDs have multiple filename variants
/// (e.g. ID 33 → "OhDear" / "Oh Dear" / "OhDeer").
///
/// WAV files live under `\user\speech\<voice_bank>\<filename>.wav`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SpeechLineId {
    Amazing = 1,
    Boring = 2,
    Brilliant = 3,
    Bummer = 4,
    Bungee = 5,
    ByeBye = 6,
    Collect = 7,
    ComeOnThen = 8,
    Coward = 9,
    DragonPunch = 10,
    Drop = 11,
    Excellent = 12,
    Fatality = 13,
    Fire = 14,
    FireBall = 15,
    FirstBlood = 16,
    Flawless = 17,
    GoAway = 18,
    Grenade = 19,
    Hello = 20,
    // 21 is unused (gap in table)
    Hurry = 22,
    IllGetYou = 23,
    Incoming = 24,
    Jump1 = 25,
    Jump2 = 26,
    JustYouWait = 27,
    Kamikaze = 28,
    Laugh = 29,
    LeaveMeAlone = 30,
    Missed = 31,
    Nooo = 32,
    OhDear = 33,
    OiNutter = 34,
    Ooff1 = 35,
    Ooff2 = 36,
    Ooff3 = 37,
    Ow1 = 38,
    Ow2 = 39,
    Ow3 = 40,
    Oops = 41,
    Orders = 42,
    Ouch = 43,
    Perfect = 44,
    Revenge = 45,
    RunAway = 46,
    Stupid = 47,
    TakeCover = 48,
    Traitor = 49,
    UhOh = 50,
    Victory = 51,
    WatchThis = 52,
    WhatThe = 53,
    // 54 is unused (gap in table)
    YesSir = 55,
    YoullRegretThat = 56,
}

impl SpeechLineId {
    /// Primary filename for this speech line (used in WAV path construction).
    pub const fn to_filename(self) -> &'static str {
        match self {
            Self::Amazing => "Amazing",
            Self::Boring => "Boring",
            Self::Brilliant => "Brilliant",
            Self::Bummer => "Bummer",
            Self::Bungee => "Bungee",
            Self::ByeBye => "ByeBye",
            Self::Collect => "Collect",
            Self::ComeOnThen => "ComeOnThen",
            Self::Coward => "Coward",
            Self::DragonPunch => "DragonPunch",
            Self::Drop => "Drop",
            Self::Excellent => "Excellent",
            Self::Fatality => "Fatality",
            Self::Fire => "Fire",
            Self::FireBall => "FireBall",
            Self::FirstBlood => "FirstBlood",
            Self::Flawless => "Flawless",
            Self::GoAway => "GoAway",
            Self::Grenade => "Grenade",
            Self::Hello => "Hello",
            Self::Hurry => "Hurry",
            Self::IllGetYou => "IllGetYou",
            Self::Incoming => "Incoming",
            Self::Jump1 => "Jump1",
            Self::Jump2 => "Jump2",
            Self::JustYouWait => "JustYouWait",
            Self::Kamikaze => "Kamikaze",
            Self::Laugh => "Laugh",
            Self::LeaveMeAlone => "LeaveMeAlone",
            Self::Missed => "Missed",
            Self::Nooo => "Nooo",
            Self::OhDear => "OhDear",
            Self::OiNutter => "OiNutter",
            Self::Ooff1 => "ooff1",
            Self::Ooff2 => "ooff2",
            Self::Ooff3 => "ooff3",
            Self::Ow1 => "ow1",
            Self::Ow2 => "ow2",
            Self::Ow3 => "ow3",
            Self::Oops => "Oops",
            Self::Orders => "Orders",
            Self::Ouch => "Ouch",
            Self::Perfect => "Perfect",
            Self::Revenge => "Revenge",
            Self::RunAway => "RunAway",
            Self::Stupid => "Stupid",
            Self::TakeCover => "TakeCover",
            Self::Traitor => "Traitor",
            Self::UhOh => "Uh-Oh",
            Self::Victory => "Victory",
            Self::WatchThis => "WatchThis",
            Self::WhatThe => "WhatThe",
            Self::YesSir => "YesSir",
            Self::YoullRegretThat => "YoullRegretThat",
        }
    }

    /// All filename variants for this speech line, including alternates.
    ///
    /// WA.exe's loader (0x571660) iterates the table at 0x6AF770 and tries
    /// each filename variant. The first file found on disk is used.
    pub const fn filenames(self) -> &'static [&'static str] {
        match self {
            Self::Amazing => &["Amazing"],
            Self::Boring => &["Boring"],
            Self::Brilliant => &["Brilliant"],
            Self::Bummer => &["Bummer"],
            Self::Bungee => &["Bungee"],
            Self::ByeBye => &["ByeBye"],
            Self::Collect => &["Collect"],
            Self::ComeOnThen => &["ComeOnThen"],
            Self::Coward => &["Coward"],
            Self::DragonPunch => &["DragonPunch"],
            Self::Drop => &["Drop"],
            Self::Excellent => &["Excellent"],
            Self::Fatality => &["Fatality"],
            Self::Fire => &["Fire"],
            Self::FireBall => &["FireBall"],
            Self::FirstBlood => &["FirstBlood"],
            Self::Flawless => &["Flawless"],
            Self::GoAway => &["GoAway"],
            Self::Grenade => &["Grenade"],
            Self::Hello => &["Hello"],
            Self::Hurry => &["Hurry"],
            Self::IllGetYou => &["IllGetYou"],
            Self::Incoming => &["Incoming"],
            Self::Jump1 => &["Jump1"],
            Self::Jump2 => &["Jump2"],
            Self::JustYouWait => &["JustYouWait"],
            Self::Kamikaze => &["Kamikaze"],
            Self::Laugh => &["Laugh"],
            Self::LeaveMeAlone => &["LeaveMeAlone"],
            Self::Missed => &["Missed"],
            Self::Nooo => &["Nooo"],
            Self::OhDear => &["OhDear", "Oh Dear", "OhDeer"],
            Self::OiNutter => &["OiNutter"],
            Self::Ooff1 => &["ooff1", "oof1"],
            Self::Ooff2 => &["ooff2", "oof2"],
            Self::Ooff3 => &["ooff3", "oof3"],
            Self::Ow1 => &["ow1"],
            Self::Ow2 => &["ow2"],
            Self::Ow3 => &["ow3"],
            Self::Oops => &["Oops"],
            Self::Orders => &["Orders"],
            Self::Ouch => &["Ouch"],
            Self::Perfect => &["Perfect"],
            Self::Revenge => &["Revenge"],
            Self::RunAway => &["RunAway"],
            Self::Stupid => &["Stupid"],
            Self::TakeCover => &["TakeCover", "TakeOver"],
            Self::Traitor => &["Traitor"],
            Self::UhOh => &["Uh-Oh", "UhOh"],
            Self::Victory => &["Victory"],
            Self::WatchThis => &["WatchThis"],
            Self::WhatThe => &["WhatThe"],
            Self::YesSir => &["YesSir"],
            Self::YoullRegretThat => &["YoullRegretThat"],
        }
    }
}

impl TryFrom<u32> for SpeechLineId {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Amazing),
            2 => Ok(Self::Boring),
            3 => Ok(Self::Brilliant),
            4 => Ok(Self::Bummer),
            5 => Ok(Self::Bungee),
            6 => Ok(Self::ByeBye),
            7 => Ok(Self::Collect),
            8 => Ok(Self::ComeOnThen),
            9 => Ok(Self::Coward),
            10 => Ok(Self::DragonPunch),
            11 => Ok(Self::Drop),
            12 => Ok(Self::Excellent),
            13 => Ok(Self::Fatality),
            14 => Ok(Self::Fire),
            15 => Ok(Self::FireBall),
            16 => Ok(Self::FirstBlood),
            17 => Ok(Self::Flawless),
            18 => Ok(Self::GoAway),
            19 => Ok(Self::Grenade),
            20 => Ok(Self::Hello),
            22 => Ok(Self::Hurry),
            23 => Ok(Self::IllGetYou),
            24 => Ok(Self::Incoming),
            25 => Ok(Self::Jump1),
            26 => Ok(Self::Jump2),
            27 => Ok(Self::JustYouWait),
            28 => Ok(Self::Kamikaze),
            29 => Ok(Self::Laugh),
            30 => Ok(Self::LeaveMeAlone),
            31 => Ok(Self::Missed),
            32 => Ok(Self::Nooo),
            33 => Ok(Self::OhDear),
            34 => Ok(Self::OiNutter),
            35 => Ok(Self::Ooff1),
            36 => Ok(Self::Ooff2),
            37 => Ok(Self::Ooff3),
            38 => Ok(Self::Ow1),
            39 => Ok(Self::Ow2),
            40 => Ok(Self::Ow3),
            41 => Ok(Self::Oops),
            42 => Ok(Self::Orders),
            43 => Ok(Self::Ouch),
            44 => Ok(Self::Perfect),
            45 => Ok(Self::Revenge),
            46 => Ok(Self::RunAway),
            47 => Ok(Self::Stupid),
            48 => Ok(Self::TakeCover),
            49 => Ok(Self::Traitor),
            50 => Ok(Self::UhOh),
            51 => Ok(Self::Victory),
            52 => Ok(Self::WatchThis),
            53 => Ok(Self::WhatThe),
            55 => Ok(Self::YesSir),
            56 => Ok(Self::YoullRegretThat),
            _ => Err(value),
        }
    }
}

/// Speech line table entry as stored in .rdata at 0x6AF770.
///
/// 61 entries (including filename alternates), null-terminated.
/// The loader iterates this table to find WAV files for each speech line.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SpeechLineTableEntry {
    /// Speech line ID (1-56, with gaps at 21 and 54).
    pub id: u32,
    /// Pointer to null-terminated filename string in .rdata.
    pub name_ptr: *const u8,
}

/// Number of entries in the speech line table (including filename alternates,
/// excluding the null terminator).
pub const SPEECH_LINE_TABLE_COUNT: usize = 61;

/// Speech slot table at DDGame+0x77E4.
///
/// Maps (team_index, speech_line_id) → DSSound buffer index.
/// 0x5A0 bytes = 360 DWORDs. Buffer indices are offset by +0x7F (127)
/// to separate speech buffer slots from SFX buffer slots.
///
/// Layout: 6 teams × 60 line slots. Line slot index = speech_line_id - 1
/// (but only valid for IDs that exist in the speech line table).
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct SpeechSlotTable(pub [u32; 360]);

impl SpeechSlotTable {
    pub const TEAMS: usize = 6;
    pub const LINES_PER_TEAM: usize = 60;

    /// Offset from SFX buffer indices. Speech buffer index = raw_index + BUFFER_OFFSET.
    pub const BUFFER_OFFSET: u32 = 0x7F;

    /// Get the DSSound buffer index for a team's speech line.
    ///
    /// `team` is 0-indexed (0..6), `line_id` is the raw speech line ID (1-56).
    /// Returns 0 if the slot is empty (no WAV loaded for this line).
    pub fn get(&self, team: usize, line_id: u32) -> u32 {
        if line_id == 0 || line_id > Self::LINES_PER_TEAM as u32 {
            return 0;
        }
        let idx = team * Self::LINES_PER_TEAM + (line_id as usize - 1);
        if idx < self.0.len() {
            self.0[idx]
        } else {
            0
        }
    }

    /// Set the DSSound buffer index for a team's speech line.
    pub fn set(&mut self, team: usize, line_id: u32, slot: u32) {
        if line_id == 0 || line_id > Self::LINES_PER_TEAM as u32 {
            return;
        }
        let idx = team * Self::LINES_PER_TEAM + (line_id as usize - 1);
        if idx < self.0.len() {
            self.0[idx] = slot;
        }
    }

    /// Clear all slots to 0 (no speech loaded).
    pub fn clear(&mut self) {
        self.0 = [0u32; 360];
    }
}

const _: () = assert!(core::mem::size_of::<SpeechLineTableEntry>() == 8);
const _: () = assert!(core::mem::size_of::<SpeechSlotTable>() == 0x5A0);

impl core::fmt::Debug for SpeechSlotTable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let non_zero = self.0.iter().filter(|&&v| v != 0).count();
        write!(f, "SpeechSlotTable({} slots loaded)", non_zero)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_line_id_roundtrip() {
        for id in 1..=56u32 {
            if id == 21 || id == 54 {
                assert!(SpeechLineId::try_from(id).is_err());
            } else {
                let line = SpeechLineId::try_from(id).unwrap();
                assert_eq!(line as u32, id);
                assert!(!line.to_filename().is_empty());
            }
        }
    }

    #[test]
    fn speech_slot_table_get_set() {
        let mut table = SpeechSlotTable([0u32; 360]);
        table.set(0, 1, 0x80); // team 0, Amazing
        assert_eq!(table.get(0, 1), 0x80);
        assert_eq!(table.get(0, 2), 0); // unset
        assert_eq!(table.get(1, 1), 0); // different team
    }

    #[test]
    fn speech_slot_table_bounds() {
        let table = SpeechSlotTable([0u32; 360]);
        assert_eq!(table.get(0, 0), 0); // ID 0 invalid
        assert_eq!(table.get(0, 61), 0); // out of range
        assert_eq!(table.get(7, 1), 0); // team out of range
    }

    #[test]
    fn filename_variants() {
        assert_eq!(SpeechLineId::OhDear.filenames().len(), 3);
        assert_eq!(SpeechLineId::Ooff1.filenames().len(), 2);
        assert_eq!(SpeechLineId::Amazing.filenames().len(), 1);
    }
}
