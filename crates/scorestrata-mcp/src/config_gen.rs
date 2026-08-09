//! Deterministic config generation from brief text.
//!
//! Wires the `generate_config_from_brief` tool to real logic (Phase 3 /
//! §28 Step 2). The generator parses genre, BPM, key, mood, and duration
//! hints from free-text briefs and produces a validated [`ProjectConfig`].
//!
//! The parser is deliberately conservative: it only claims what it can
//! reliably detect, and falls back to sensible defaults otherwise.

use scorestrata_core::domain::{DeliveryTarget, ProjectConfig, TimeSignature};

/// A brief parsed into structured hints.
#[derive(Debug, Clone, Default)]
pub struct BriefHints {
    /// Detected genre (e.g. `melodic_techno`).
    pub genre: Option<String>,
    /// Detected BPM.
    pub bpm: Option<u32>,
    /// Detected musical key (e.g. "C minor").
    pub key: Option<String>,
    /// Detected mood tags.
    pub mood: Vec<String>,
    /// Detected delivery target.
    pub delivery_target: Option<DeliveryTarget>,
    /// Detected duration in bars.
    pub duration_bars: Option<u32>,
    /// Detected time signature numerator.
    pub time_sig_num: Option<u32>,
}

/// Detect genre keywords in brief text.
fn detect_genre(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let candidates: &[(&str, &[&str])] = &[
        ("melodic_techno", &["melodic techno", "melodic house"]),
        ("techno", &["techno", "dark techno", "minimal techno"]),
        ("house", &["house", "deep house", "tech house", "garage"]),
        (
            "ambient",
            &["ambient", "drone", "chillout", "new age", "meditation"],
        ),
        (
            "cinematic",
            &["cinematic", "film score", "orchestral", "epic trailer"],
        ),
        ("trance", &["trance", "psytrance", "uplifting"]),
        ("dubstep", &["dubstep", "bass music", "riddim", "brostep"]),
        ("lo-fi", &["lo-fi", "lofi", "chill hop", "study beats"]),
        (
            "drum_and_bass",
            &[
                "drum and bass",
                "drum_and_bass",
                "dnb",
                "jungle",
                "breakbeat",
            ],
        ),
        (
            "nu_metal",
            &["nu metal", "nu-metal", "linkin park", "rap metal"],
        ),
        (
            "rock",
            &[
                "rock",
                "hard rock",
                "punk",
                "grunge",
                "metal",
                "heavy metal",
            ],
        ),
        (
            "jazz",
            &[
                "jazz",
                "swing",
                "bossa nova",
                "bebop",
                "smooth jazz",
                "big band",
            ],
        ),
        (
            "hip-hop",
            &[
                "hip-hop", "hip hop", "hiphop", "trap", "drill", "boom bap", "rap", "grime",
                "phonk",
            ],
        ),
        (
            "pop",
            &[
                "pop",
                "k-pop",
                "j-pop",
                "synth-pop",
                "indie pop",
                "dance pop",
            ],
        ),
        ("funk", &["funk", "soul", "disco", "groove"]),
        ("blues", &["blues", "delta blues", "electric blues"]),
        (
            "synthwave",
            &["synthwave", "retrowave", "outrun", "vaporwave", "80s synth"],
        ),
        (
            "classical",
            &["classical", "baroque", "romantic", "piano sonata"],
        ),
        ("reggae", &["reggae", "dub music", "dancehall"]),
        ("country", &["country", "bluegrass", "americana"]),
        ("folk", &["folk", "acoustic", "singer-songwriter"]),
        (
            "latin",
            &["latin", "salsa", "reggaeton", "flamenco", "tango", "samba"],
        ),
        ("gospel", &["gospel", "worship", "choir"]),
        (
            "electronic",
            &[
                "electronic",
                "edm",
                "electro",
                "idm",
                "glitch",
                "industrial",
            ],
        ),
        (
            "world",
            &[
                "world music",
                "celtic",
                "middle eastern",
                "indian classical",
                "african",
            ],
        ),
    ];
    for (genre, keywords) in candidates {
        if keywords.iter().any(|k| lower.contains(k)) {
            return Some((*genre).to_string());
        }
    }
    None
}

/// Detect BPM: prefer an explicit "N bpm" phrase, then the first 3-digit
/// number (60–240). Numbers that are duration hints ("32 bars", "16-bar")
/// are excluded so a brief like "rock, 32 bars" cannot mis-read the bar
/// count as a tempo.
fn detect_bpm(text: &str) -> Option<u32> {
    let lower = text.to_lowercase();
    // 1. Explicit "<digits> bpm" wins outright.
    for token in lower.split(|c: char| !c.is_ascii_alphanumeric() && c != ' ') {
        let token = token.trim();
        if token.ends_with("bpm") {
            let num: String = token.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num.parse::<u32>()
                && (40..=240).contains(&n)
            {
                return Some(n);
            }
        }
    }
    // 2. Fall back to the first 3-digit number, skipping "N bar(s)" hints
    //    (a duration "32 bars" must not be read as a 32 BPM tempo).
    let bytes = lower.as_bytes();
    let mut best: Option<u32> = None;
    let mut i = 0usize;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let token = &lower[start..i];
        if let Ok(n) = token.parse::<u32>() {
            let rest = lower[i..].trim_start_matches([' ', '-', '_', '\n', '\t']);
            let is_bar_count = rest.starts_with("bar");
            if !is_bar_count && (100..=240).contains(&n) {
                if best.is_none_or(|b| !(100..=240).contains(&b)) {
                    best = Some(n);
                }
            }
        }
    }
    best
}

/// Detect a key like "C minor", "A# Major", "G mixolydian".
fn detect_key(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    const ROOTS: &[&str] = &[
        "c#", "db", "d#", "eb", "f#", "gb", "g#", "ab", "a#", "bb", "c", "d", "e", "f", "g", "a",
        "b",
    ];
    const MODES: &[&str] = &[
        "minor",
        "major",
        "dorian",
        "mixolydian",
        "maj",
        "min",
        "moll",
        "dur",
    ];
    for mode in MODES {
        if let Some(idx) = lower.find(mode) {
            let prefix = &lower[..idx];
            let root = prefix
                .split(|c: char| !c.is_ascii_alphabetic() && c != '#')
                .filter(|s| !s.is_empty())
                .last();
            if let Some(root) = root
                && ROOTS.contains(&root)
            {
                let canonical = {
                    let mut chars = root.chars();
                    let first = chars.next()?.to_uppercase().to_string();
                    format!("{first}{}", chars.as_str())
                };
                let canonical_mode = match *mode {
                    "minor" | "min" | "moll" => "minor".to_string(),
                    "major" | "maj" | "dur" => "Major".to_string(),
                    "dorian" => "Dorian".to_string(),
                    "mixolydian" => "Mixolydian".to_string(),
                    _ => continue,
                };
                return Some(format!("{canonical} {canonical_mode}"));
            }
        }
    }
    None
}

/// Detect mood tags from a curated keyword map.
fn detect_mood(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    const MOODS: &[(&str, &str)] = &[
        ("dark", "dark"),
        ("driving", "driving"),
        ("emotional", "emotional"),
        ("uplifting", "uplifting"),
        ("energetic", "energetic"),
        ("melancholic", "melancholic"),
        ("dreamy", "dreamy"),
        ("aggressive", "aggressive"),
        ("chill", "chill"),
        ("ethereal", "ethereal"),
        ("tense", "tense"),
        ("joyful", "joyful"),
    ];
    let mut found = Vec::new();
    for (keyword, tag) in MOODS {
        if lower.contains(keyword) && !found.contains(&(*tag).to_string()) {
            found.push((*tag).to_string());
        }
    }
    found
}

/// Detect delivery target keywords.
fn detect_delivery(text: &str) -> Option<DeliveryTarget> {
    let lower = text.to_lowercase();
    if lower.contains("club") || lower.contains("dj") || lower.contains("dancefloor") {
        Some(DeliveryTarget::Club)
    } else if lower.contains("cd") || lower.contains("album") {
        Some(DeliveryTarget::Cd)
    } else if lower.contains("broadcast") || lower.contains("tv") {
        Some(DeliveryTarget::Broadcast)
    } else if lower.contains("streaming") || lower.contains("spotify") {
        Some(DeliveryTarget::Streaming)
    } else {
        None
    }
}

/// Detect duration in bars: patterns like "64-bar", "8 bars", "16 bar loop".
fn detect_duration_bars(text: &str) -> Option<u32> {
    let lower = text.to_lowercase();
    for token in lower.split(|c: char| !c.is_ascii_alphanumeric() && c != ' ') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if token.ends_with("bar") {
            let num: String = token.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num.parse::<u32>()
                && (1..=256).contains(&n)
            {
                return Some(n);
            }
        }
    }
    // "8-bar loop" or "64 bars"
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    for (i, token) in tokens.iter().enumerate() {
        if let Ok(n) = token.parse::<u32>()
            && (1..=256).contains(&n)
            && tokens.get(i + 1).is_some_and(|t| t.starts_with("bar"))
        {
            return Some(n);
        }
    }
    None
}

/// Detect time signature hints like "3/4" or "7/8".
fn detect_time_sig(text: &str) -> Option<u32> {
    for token in text.split(|c: char| !c.is_ascii_alphanumeric() && c != '/') {
        if let Some((num, _den)) = token.split_once('/') {
            if let Ok(n) = num.trim().parse::<u32>()
                && (1..=12).contains(&n)
            {
                return Some(n);
            }
        }
    }
    None
}

/// Parse brief text into structured hints.
#[must_use]
pub fn parse_brief(text: &str) -> BriefHints {
    BriefHints {
        genre: detect_genre(text),
        bpm: detect_bpm(text),
        key: detect_key(text),
        mood: detect_mood(text),
        delivery_target: detect_delivery(text),
        duration_bars: detect_duration_bars(text),
        time_sig_num: detect_time_sig(text),
    }
}

/// Generate a [`ProjectConfig`] from brief text with a project name.
#[must_use]
pub fn generate_config(project_name: &str, brief_text: &str) -> ProjectConfig {
    let hints = parse_brief(brief_text);

    // Sensible genre-default BPM/key mapping when not detected.
    let (default_bpm, default_key) = match hints.genre.as_deref() {
        Some("techno") | Some("melodic_techno") => (124, "C minor"),
        Some("house") => (122, "G minor"),
        Some("ambient") => (80, "A minor"),
        Some("cinematic") => (90, "D minor"),
        Some("trance") => (138, "F minor"),
        Some("nu_metal") => (104, "E minor"),
        Some("rock") => (112, "A minor"),
        Some("jazz") => (112, "C major"),
        Some("hip-hop") => (92, "E minor"),
        Some("pop") => (118, "C major"),
        Some("funk") => (104, "G minor"),
        Some("blues") => (96, "E minor"),
        Some("synthwave") => (110, "A minor"),
        Some("drum_and_bass") => (172, "D minor"),
        Some("dubstep") => (140, "F minor"),
        Some("lo-fi") => (84, "A minor"),
        Some("classical") => (72, "C major"),
        Some("reggae") => (82, "A major"),
        Some("country") => (100, "G major"),
        Some("folk") => (96, "C major"),
        Some("latin") => (104, "D minor"),
        Some("gospel") => (88, "C major"),
        Some("electronic") => (126, "C minor"),
        Some("world") => (90, "D major"),
        _ => (120, "C minor"),
    };

    ProjectConfig {
        name: project_name.to_string(),
        genre: hints
            .genre
            .clone()
            .unwrap_or_else(|| "experimental".to_string()),
        bpm: hints.bpm.unwrap_or(default_bpm),
        key: hints.key.clone().unwrap_or_else(|| default_key.to_string()),
        time_signature: TimeSignature {
            numerator: hints.time_sig_num.unwrap_or(4),
            denominator: 4,
        },
        duration_bars: hints.duration_bars.unwrap_or(8),
        delivery_target: hints.delivery_target.unwrap_or(DeliveryTarget::Streaming),
        mood: hints.mood,
        references: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_brief() {
        let hints = parse_brief("Dark melodic techno at 124 BPM in C minor, 64 bars");
        assert_eq!(hints.genre.as_deref(), Some("melodic_techno"));
        assert_eq!(hints.bpm, Some(124));
        assert_eq!(hints.key.as_deref(), Some("C minor"));
        assert_eq!(hints.duration_bars, Some(64));
        assert!(hints.mood.contains(&"dark".to_string()));
    }

    #[test]
    fn parses_ambient_brief() {
        let hints = parse_brief("Chill ambient drone, 80 bpm, A minor, 32 bars");
        assert_eq!(hints.genre.as_deref(), Some("ambient"));
        assert_eq!(hints.bpm, Some(80));
        assert_eq!(hints.key.as_deref(), Some("A minor"));
        assert_eq!(hints.duration_bars, Some(32));
    }

    #[test]
    fn detects_key_with_sharp_root() {
        let hints = parse_brief("Aggressive dubstep in A# minor");
        assert_eq!(hints.key.as_deref(), Some("A# minor"));
        assert_eq!(hints.genre.as_deref(), Some("dubstep"));
    }

    #[test]
    fn detects_multiword_forms() {
        let hints = parse_brief("Uplifting house groove 126 BPM G Major 8 bar loop");
        assert_eq!(hints.genre.as_deref(), Some("house"));
        assert_eq!(hints.bpm, Some(126));
        assert_eq!(hints.key.as_deref(), Some("G Major"));
        assert_eq!(hints.duration_bars, Some(8));
    }

    #[test]
    fn detects_club_delivery() {
        let hints = parse_brief("Club-oriented techno");
        assert_eq!(hints.delivery_target, Some(DeliveryTarget::Club));
    }

    #[test]
    fn detects_time_signature() {
        let hints = parse_brief("Cinematic 7/8 track");
        assert_eq!(hints.time_sig_num, Some(7));
    }

    #[test]
    fn generate_config_has_sensible_defaults() {
        let config = generate_config("My Track", "Dark melodic techno");
        assert_eq!(config.name, "My Track");
        assert_eq!(config.genre, "melodic_techno");
        assert_eq!(config.bpm, 124);
        assert_eq!(config.key, "C minor");
        assert_eq!(config.duration_bars, 8);
        assert_eq!(config.time_signature.numerator, 4);
    }

    #[test]
    fn generate_config_uses_detected_values() {
        let config = generate_config(
            "Ambient Study",
            "Chill ambient, 90 bpm, D minor, 32 bars, dreamy",
        );
        assert_eq!(config.bpm, 90);
        assert_eq!(config.key, "D minor");
        assert_eq!(config.duration_bars, 32);
        assert!(config.mood.contains(&"dreamy".to_string()));
    }

    #[test]
    fn generate_config_serializes() {
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&generate_config("T", "Techno 132")).unwrap();
        assert!(json.contains("\"bpm\": 132"));
    }

    #[test]
    fn empty_brief_uses_defaults() {
        let config = generate_config("Untitled", "");
        assert_eq!(config.genre, "experimental");
        assert_eq!(config.bpm, 120);
        assert_eq!(config.duration_bars, 8);
    }

    #[test]
    fn bar_count_is_not_misread_as_bpm() {
        // "32 bars" is a duration hint, not a 32 BPM tempo. The nu-metal
        // default (104) must survive the bar count in the brief.
        let config = generate_config(
            "Crawling",
            "linkin park style nu metal rock, 32 bars, heavy guitars",
        );
        assert_eq!(config.genre, "nu_metal");
        assert_eq!(config.bpm, 104, "duration bar count not read as tempo");
        assert_eq!(config.duration_bars, 32);

        // An explicit tempo still wins.
        let explicit = generate_config("Riff", "hard rock, 120 bpm, 32 bars");
        assert_eq!(explicit.bpm, 120);
        assert_eq!(explicit.duration_bars, 32);

        // A multi-line brief with the bar count on its own line must not
        // be read as a tempo either (the "N bar" lookahead trims newlines).
        let multiline = generate_config("Riff", "rock\n128 bars");
        assert_eq!(multiline.genre, "rock");
        assert_eq!(multiline.bpm, 112, "multi-line bar count not read as tempo");
        assert_eq!(multiline.duration_bars, 128);
    }

    #[test]
    fn s3_detects_all_new_genre_families() {
        // Every S3 family must be detected from a natural brief and get its
        // own BPM/key defaults — not the generic experimental fallback.
        let cases = [
            ("jazz swing quartet", "jazz", 112),
            ("hip-hop trap beat", "hip-hop", 92),
            ("pop song for radio", "pop", 118),
            ("funky disco groove", "funk", 104),
            ("delta blues shuffle", "blues", 96),
            ("synthwave retrowave", "synthwave", 110),
            ("drum and bass jungle", "drum_and_bass", 172),
            ("lo-fi chill hop", "lo-fi", 84),
            ("classical piano", "classical", 72),
            ("reggae dub", "reggae", 82),
            ("country ballad", "country", 100),
            ("folk acoustic", "folk", 96),
            ("latin salsa", "latin", 104),
            ("gospel choir", "gospel", 88),
            ("edm electronic", "electronic", 126),
        ];
        for (brief, expected_genre, expected_bpm) in cases {
            let config = generate_config("S3", brief);
            assert_eq!(
                config.genre, expected_genre,
                "brief '{brief}' should detect '{expected_genre}', got '{}'",
                config.genre
            );
            assert_eq!(
                config.bpm, expected_bpm,
                "brief '{brief}' should default to {expected_bpm} BPM"
            );
        }
    }

    #[test]
    fn detects_rock_and_nu_metal_briefs() {
        // The Linkin Park stress test: a nu-metal brief must land on the
        // rock family with a rock tempo (104 BPM, E minor), not fall back
        // to the generic "experimental" baseline.
        let lp = generate_config(
            "Crawling",
            "linkin park style nu metal rock with heavy distorted guitars",
        );
        assert_eq!(lp.genre, "nu_metal");
        assert_eq!(lp.bpm, 104);
        assert_eq!(lp.key, "E minor");

        let rock = generate_config("Riff", "Hard rock, aggressive, 120 bpm");
        assert_eq!(rock.genre, "rock");
        assert_eq!(rock.bpm, 120);
        assert_eq!(rock.key, "A minor");
    }
}
