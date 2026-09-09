use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use crate::types::ComicBook;

struct Patterns {
    tags: Regex,
    handles: Regex,
    issue: Regex,
    leading_number: Regex,
    trailing_number: Regex,
    edition: Regex,
    extra: Regex,
    bracket_number: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| Patterns {
        tags: Regex::new(r"\([^)]*\)|\[[^\]]*\]|\{[^}]*\}").unwrap(),
        handles: Regex::new(r"@\S+").unwrap(),
        issue: Regex::new(r"(?i)(?:^|[\s–—|:#-])(?P<kind>chapter|chap|ch|volume|vol|v|issue|no|episode|ep|part|c)\.?\s*[#:_-]?\s*(?P<number>[a-z]?\d+(?:\.\d+)?(?:-\d+(?:\.\d+)?)?)\b").unwrap(),
        leading_number: Regex::new(r"^\s*(\d+(?:\.\d+)?)\s*[-–—|:]\s*").unwrap(),
        trailing_number: Regex::new(r"(?:\s+|[-–—|:]+)\s*#?(\d+(?:\.\d+)?(?:-\d+(?:\.\d+)?)?)$").unwrap(),
        edition: Regex::new(r"(?i)\s+(?:complete|completed|colored|coloured|color|colour|digital)$").unwrap(),
        extra: Regex::new(r"(?i)\s*[-–—|:]\s*(?:extra\b|ex\s+special\b|special\s+bonus\b)").unwrap(),
        bracket_number: Regex::new(r"^\s*(?:\[[^\]]+\]\s*)*?\[(\d+(?:\.\d+)?)\]").unwrap(),
    })
}

fn clean(title: &str) -> String {
    let patterns = patterns();
    let title = patterns.tags.replace_all(title, " ");
    let title = patterns.handles.replace_all(&title, " ");
    title
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn trim_edges(value: &str) -> &str {
    value
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '–' | '—' | '|' | ':' | '_'))
}

/// None means that the filename provides only chapter/release information;
/// its folder is needed to identify the series.
fn parsed_series(title: &str) -> Option<String> {
    let patterns = patterns();
    let cleaned = clean(title);
    let mut name = trim_edges(&cleaned);
    loop {
        if let Some(prefix) = patterns.issue.find(name).filter(|m| m.start() == 0) {
            name = trim_edges(&name[prefix.end()..]);
        } else if let Some(prefix) = patterns.leading_number.find(name) {
            name = trim_edges(&name[prefix.end()..]);
        } else {
            break;
        }
    }
    // Explicit chapter/volume markers also precede chapter subtitles.
    if let Some(issue) = patterns.issue.find(name) {
        name = trim_edges(&name[..issue.start()]);
    }
    if let Some(extra) = patterns.extra.find(name) {
        name = trim_edges(&name[..extra.start()]);
    }
    loop {
        if let Some(tail) = patterns.trailing_number.find(name) {
            name = trim_edges(&name[..tail.start()]);
        } else if let Some(tail) = patterns.edition.find(name) {
            name = trim_edges(&name[..tail.start()]);
        } else {
            break;
        }
    }
    (!name.is_empty() && name.chars().any(char::is_alphabetic)).then(|| name.to_string())
}

#[cfg(test)]
pub fn extract_series(title: &str) -> String {
    parsed_series(title).unwrap_or_else(|| title.trim().to_string())
}

pub fn series_key(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn folder_hint<'a>(folders: impl DoubleEndedIterator<Item = &'a str>) -> Option<String> {
    folders.rev().filter_map(parsed_series).find(|name| {
        !matches!(
            series_key(name).as_str(),
            "manga"
                | "mangas"
                | "comic"
                | "comics"
                | "comic books"
                | "books"
                | "collection"
                | "my collection"
                | "library"
                | "downloads"
                | "google drive"
                | "drive"
                | "digital"
                | "english"
                | "cbz"
                | "cbr"
                | "cb7"
                | "pdf"
                | "zip"
                | "chapters"
                | "volumes"
                | "complete"
                | "completed"
                | "ongoing"
                | "color"
                | "colored"
                | "coloured"
                | "raw"
                | "raws"
        )
    })
}

pub fn series_for(title: &str, hint: Option<&str>) -> String {
    let parsed = parsed_series(title);
    match (parsed, hint) {
        (None, Some(folder)) => folder.to_string(),
        (Some(name), Some(folder)) => {
            let name_key = series_key(&name);
            let folder_key = series_key(folder);
            // A truncated filename can omit a subtitle/series prefix. Folder
            // context disambiguates e.g. Ragnarok from the original series.
            if folder_key.starts_with(&format!("{name_key} "))
                || folder_key.ends_with(&format!(" {name_key}"))
            {
                folder.to_string()
            } else {
                name
            }
        }
        (Some(name), None) => name,
        (None, None) => title.trim().to_string(),
    }
}

fn display_name(value: &str) -> String {
    if value != value.to_lowercase() && value != value.to_uppercase() {
        return value.to_string();
    }
    value
        .split_whitespace()
        .enumerate()
        .map(|(index, word)| {
            let lower = word.to_lowercase();
            if index > 0
                && matches!(
                    lower.as_str(),
                    "a" | "an"
                        | "and"
                        | "as"
                        | "at"
                        | "by"
                        | "for"
                        | "in"
                        | "of"
                        | "on"
                        | "or"
                        | "the"
                        | "to"
                        | "with"
                        | "x"
                )
            {
                return lower;
            }
            let mut letters = lower.chars();
            letters
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + letters.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Canonicalize case/punctuation variants together so the grid, series view,
/// search, and chapter navigation agree on a single series name.
pub fn regroup(comics: &mut [ComicBook]) {
    let mut names = HashMap::<String, String>::new();
    for comic in comics.iter_mut() {
        comic.series = series_for(&comic.title, comic.series_hint.as_deref());
        let key = series_key(&comic.series);
        let candidate = display_name(&comic.series);
        let current = names.entry(key).or_insert_with(|| candidate.clone());
        let score = |s: &str| s.chars().filter(|c| c.is_uppercase()).count();
        if score(&candidate) > score(current)
            || (score(&candidate) == score(current) && candidate < *current)
        {
            *current = candidate;
        }
    }
    for comic in comics {
        if let Some(name) = names.get(&series_key(&comic.series)) {
            comic.series = name.clone();
        }
    }
}

fn issue_number(title: &str) -> Option<String> {
    let patterns = patterns();
    let cleaned = clean(title);
    let captures: Vec<_> = patterns.issue.captures_iter(&cleaned).collect();
    // Prefer a chapter/issue when a filename also specifies a volume.
    if let Some(capture) = captures
        .iter()
        .find(|c| {
            !matches!(
                &c["kind"].to_lowercase()[..],
                "volume" | "vol" | "v" | "part"
            )
        })
        .or_else(|| captures.first())
    {
        return Some(capture["number"].to_lowercase());
    }
    for expression in [&patterns.leading_number, &patterns.trailing_number] {
        if let Some(capture) = expression.captures(trim_edges(&cleaned)) {
            return Some(capture[1].to_string());
        }
    }
    patterns
        .bracket_number
        .captures(title)
        .map(|capture| capture[1].to_string())
}

fn volume_number(title: &str) -> String {
    patterns()
        .issue
        .captures_iter(&clean(title))
        .find(|c| matches!(&c["kind"].to_lowercase()[..], "volume" | "vol" | "v"))
        .map(|c| c["number"].to_lowercase())
        .unwrap_or_else(|| "0".into())
}

fn compare_numbers(a: &str, b: &str) -> std::cmp::Ordering {
    fn parts(value: &str) -> (&str, &str, &str) {
        let value = value.split('-').next().unwrap_or(value);
        let prefix_end = value.find(|c: char| c.is_ascii_digit()).unwrap_or(0);
        let (prefix, number) = value.split_at(prefix_end);
        let (integer, fraction) = number.split_once('.').unwrap_or((number, ""));
        (
            prefix,
            integer.trim_start_matches('0'),
            fraction.trim_end_matches('0'),
        )
    }
    let (ap, ai, af) = parts(a);
    let (bp, bi, bf) = parts(b);
    // Lettered prologues (a0, b0…) precede the numbered chapters. Ignore
    // zero padding; natural string order otherwise places 010 before 2.
    ap.is_empty()
        .cmp(&bp.is_empty())
        .then_with(|| ap.cmp(bp))
        .then_with(|| ai.len().cmp(&bi.len()))
        .then_with(|| ai.cmp(bi))
        .then_with(|| af.cmp(bf))
}

pub fn compare_chapters(a: &str, b: &str) -> std::cmp::Ordering {
    compare_numbers(&volume_number(a), &volume_number(b))
        .then_with(|| match (issue_number(a), issue_number(b)) {
            (Some(a), Some(b)) => compare_numbers(&a, &b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
        .then_with(|| natord::compare_ignore_case(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_chapter_prefixes_subtitles_and_release_tags() {
        for (title, expected) in [
            ("CH209 - Chainsaw Man [@The_Mates]", "Chainsaw Man"),
            ("c112 - Chainsaw Man", "Chainsaw Man"),
            ("Chainsaw Man Part 1 Public Safety Arc", "Chainsaw Man"),
            (
                "Chapter 02 - Solo Leveling_ Ragnarok",
                "Solo Leveling Ragnarok",
            ),
            ("20 - Solo Leveling Ragnarok", "Solo Leveling Ragnarok"),
            (
                "chapter 56 - Solo Leveling Ragnarok",
                "Solo Leveling Ragnarok",
            ),
            ("Jujutsu Kaisen Chapter 10_After the Rain", "Jujutsu Kaisen"),
            ("Jujutsu Kaisen Chapter13", "Jujutsu Kaisen"),
            ("Jujutsu Kaisen - extra five page bonus", "Jujutsu Kaisen"),
            ("Jujutsu Kaisen- ex Special Bonus", "Jujutsu Kaisen"),
            ("berserk chapter a0", "berserk"),
            ("Berserk_complete", "Berserk"),
            ("Kaguya Sama colored", "Kaguya Sama"),
            (
                "Batman Issue 004 - A New Beginning (Digital Edition)",
                "Batman",
            ),
        ] {
            assert_eq!(extract_series(title), expected, "{title}");
        }
    }

    #[test]
    fn uses_folder_context_for_unnamed_chapters_without_merging_distinct_titles() {
        let hint =
            folder_hint(["Manga", "Solo Leveling Ragnarok", "Volume 01"].into_iter()).unwrap();
        assert_eq!(hint, "Solo Leveling Ragnarok");
        for title in [
            "[Manga Universe] Chapter 06",
            "Chapter 47 @solo_leveling_eng",
            "Chapter 48 - Solo Leveling",
            "Chapter 45 - @solo_leveling_eng Ragnarok",
        ] {
            assert_eq!(series_for(title, Some(&hint)), hint, "{title}");
        }
        assert_eq!(
            series_for("Record of Ragnarok v15", Some(&hint)),
            "Record of Ragnarok"
        );
        assert_eq!(
            folder_hint(["Manga", "Downloads", "Chapter 01"].into_iter()),
            None
        );
        assert_eq!(extract_series("2000 AD 1234"), "2000 AD");
        assert_eq!(extract_series("20th Century Boys 12"), "20th Century Boys");
    }

    #[test]
    fn mixed_filename_styles_sort_in_chapter_order() {
        let mut titles = [
            "CH209 - Chainsaw Man",
            "Chainsaw Man - 002 [Color]",
            "c112 - Chainsaw Man",
            "Chainsaw Man - 001 [Color]",
        ];
        titles.sort_by(|a, b| compare_chapters(a, b));
        assert_eq!(
            titles,
            [
                "Chainsaw Man - 001 [Color]",
                "Chainsaw Man - 002 [Color]",
                "c112 - Chainsaw Man",
                "CH209 - Chainsaw Man"
            ]
        );
        assert!(compare_chapters("Spy x Family 098.1", "Spy x Family 099").is_lt());
        assert!(compare_chapters("Chapter 2 - Saga", "Saga 010").is_lt());
        assert!(compare_chapters("Saga 098.2", "Saga 098.11").is_gt());
        assert!(compare_chapters("Berserk chapter a0", "Berserk chapter 1").is_lt());
        assert!(compare_chapters("Saga v1 c10", "Saga v2 c1").is_lt());
    }
}
