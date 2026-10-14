use crate::{Song, Verse};

/// Hardcoded Phase 0 content: "Blessed Assurance" (Fanny Crosby, 1873), public domain.
/// The short ending slide exercises auto-fit at the large end of the size range.
pub fn sample_song() -> Song {
    Song {
        id: 1,
        title: "Blessed Assurance".into(),
        alt_title: None,
        authors: vec!["Fanny J. Crosby".into()],
        copyright: Some("Public domain".into()),
        ccli_number: None,
        verses: vec![
            Verse::new(
                "v1",
                "Blessed assurance, Jesus is mine!
                 O what a foretaste of glory divine!
                 Heir of salvation, purchase of God,
                 Born of His Spirit, washed in His blood.",
            ),
            Verse::new(
                "c1",
                "This is my story, this is my song,
                 Praising my Savior all the day long;
                 This is my story, this is my song,
                 Praising my Savior all the day long.",
            ),
            Verse::new(
                "v2",
                "Perfect submission, perfect delight,
                 Visions of rapture now burst on my sight;
                 Angels descending bring from above
                 Echoes of mercy, whispers of love.",
            ),
            Verse::new(
                "v3",
                "Perfect submission, all is at rest,
                 I in my Savior am happy and blest,
                 Watching and waiting, looking above,
                 Filled with His goodness, lost in His love.",
            ),
            Verse::new("e1", "Amen."),
        ],
        verse_order: ["v1", "c1", "v2", "c1", "v3", "c1", "e1"]
            .map(String::from)
            .to_vec(),
    }
}
