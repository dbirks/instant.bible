use crate::proto::data::{decode_translation_data, Translation, VerseKey, VerseText};
use crate::{ReverseIndexEntryBytes, VersearchIndex, TRANSLATION_COUNT};
use anyhow::{Context, Result};
use fst::MapBuilder;
use log::info;
use serde::Deserialize;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::prelude::*;
use std::iter::Iterator;
use std::time::Instant;

pub static MAX_PROXIMITY: u64 = 8;

#[cfg_attr(test, derive(Debug))]
#[derive(Deserialize)]
pub struct Config {
    pub translation_dir: String,
}

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct Tokenized {
    pub source: String,
    pub token: String,
}

impl Ord for Tokenized {
    fn cmp(&self, other: &Self) -> Ordering {
        self.token.cmp(&other.token)
    }
}

impl PartialOrd for Tokenized {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct VerseStats {
    counts: Vec<usize>,
    highlights: BTreeSet<String>,
}

type TranslationVerses = BTreeMap<Translation, BTreeMap<VerseKey, String>>;

pub fn tokenize(input: &str) -> Vec<Tokenized> {
    input
        .split_whitespace()
        .map(|s| Tokenized {
            token: s
                .chars()
                // Keeping only alphanumeric characters lets users search without
                // concern for apostrophes and the like
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_uppercase(),
            source: s
                .chars()
                .enumerate()
                // Like tokens but with apostophes and commas (except trailing commas)
                .filter(|(i, c)| {
                    c.is_ascii_alphanumeric() || *c == '\'' || (*c == ',' && *i != s.len() - 1)
                })
                .map(|(_i, c)| c)
                .collect::<String>(),
        })
        .collect()
}

/// Given a translation id, a verse key, and two word ids, generates a sequence
/// of bytes which can be used as a key into an FST map
pub fn proximity_bytes_key(tidx: u8, vkey: &VerseKey, w1i: u16, w2i: u16) -> Vec<u8> {
    let capacity =
        std::mem::size_of::<u8>() + std::mem::size_of::<u16>() * 2 + VerseKey::get_byte_size();
    let mut v = Vec::with_capacity(capacity);
    v.extend(&tidx.to_be_bytes());
    v.extend(&vkey.to_be_bytes());
    v.extend(&w1i.to_be_bytes());
    v.extend(&w2i.to_be_bytes());
    v
}

/// Given a translation id and a verse key, generates a sequence of bytes which
/// can be used as a key into an FST map
pub fn translation_verses_bytes_key(tidx: u8, vkey: &VerseKey) -> Vec<u8> {
    let capacity = std::mem::size_of::<u8>() + VerseKey::get_byte_size();
    let mut v = Vec::with_capacity(capacity);
    v.extend(&tidx.to_be_bytes());
    v.extend(&vkey.to_be_bytes());
    v
}

/// Reads and returns the bytes of a file located at the given path
#[inline]
fn read_file_bytes(path: &std::path::PathBuf) -> Result<Vec<u8>> {
    let mut file_bytes = Vec::new();
    fs::File::open(path)
        .context("Could not open file")?
        .read_to_end(&mut file_bytes)
        .context("Could not read file")?;
    Ok(file_bytes)
}

/// Stores work-in-progress proximity calculations
type WipProximitiesMap =
    BTreeMap<usize, BTreeMap<VerseKey, BTreeMap<String, BTreeMap<String, u64>>>>;
// Stores work-in-progress token counts per verse and translation
type WipTokenCountsMap = BTreeMap<String, BTreeMap<VerseKey, VerseStats>>;

/// Performs initial processing of verses read from disk
#[inline]
fn process_verses(
    translation_key: Translation,
    verses: &[VerseText],
    translation_verses: &mut TranslationVerses,
    highlight_words: &mut BTreeSet<String>,
    wip_token_counts: &mut BTreeMap<String, BTreeMap<VerseKey, VerseStats>>,
    proximities: &mut WipProximitiesMap,
) {
    for verse in verses {
        translation_verses
            .entry(translation_key)
            .or_insert_with(BTreeMap::new)
            .entry(verse.key.unwrap())
            .or_insert_with(|| verse.text.clone());
        let vkey = verse.key.expect("Missing verse key");
        let verse_tokens = tokenize(&verse.text);
        // Count up tokens
        for (i, tokenized) in verse_tokens.iter().enumerate() {
            // Save word to get a highlight id later
            highlight_words.insert(tokenized.source.to_uppercase());
            // Create new stats entry if needed
            let entry = wip_token_counts
                .entry(tokenized.token.clone())
                .or_insert_with(BTreeMap::new)
                .entry(vkey.clone())
                .or_insert_with(|| VerseStats {
                    counts: vec![0; TRANSLATION_COUNT],
                    highlights: BTreeSet::new(),
                });
            // Increment counts
            entry.counts[translation_key as usize] += 1;
            // Track highlights
            entry.highlights.insert(tokenized.source.to_uppercase());
            // Track proximities
            for (j, other_tokenized) in verse_tokens.iter().enumerate().skip(i + 1) {
                let prox = (j - i) as u64;
                proximities
                    .entry(translation_key as usize)
                    .or_insert_with(BTreeMap::new)
                    .entry(vkey.clone())
                    .or_insert_with(BTreeMap::new)
                    .entry(tokenized.token.clone())
                    .or_insert_with(BTreeMap::new)
                    .entry(other_tokenized.token.clone())
                    .and_modify(|p: &mut u64| {
                        if prox < *p {
                            *p = prox;
                        } else if prox > MAX_PROXIMITY {
                            *p = MAX_PROXIMITY
                        }
                    })
                    .or_insert(prox);
            }
        }
    }
}

/// Loads data from disk and returns the total number of documents
#[inline]
fn load_data(
    translation_verses: &mut TranslationVerses,
    highlight_words: &mut BTreeSet<String>,
    wip_token_counts: &mut WipTokenCountsMap,
    proximities: &mut WipProximitiesMap,
) -> Result<()> {
    let config = envy::from_env::<Config>()?;
    info!("Loading translations from {:?}", config.translation_dir);

    let mut total_docs: usize = 0;

    for entry in
        fs::read_dir(config.translation_dir).context("Could not read translation data directory")?
    {
        let path = entry.unwrap().path();
        if path.is_file() && path.extension().map(|s| s == "pb").unwrap_or(false) {
            let translation_name = path
                .file_stem()
                .expect("Could not get file stem")
                .to_string_lossy()
                .to_string();
            info!("Load translation {:?} from {:?}", translation_name, path);
            let now = Instant::now();
            let file_bytes = read_file_bytes(&path).expect("Could not read protobuf file");
            let data = decode_translation_data(&*file_bytes).expect("Could not parse protobuf");
            let translation_key =
                Translation::from_i32(data.translation).expect("Invalid translation field value");
            info!(
                "Read {} verses in {}ms",
                data.verses.len(),
                now.elapsed().as_millis()
            );
            total_docs = total_docs.max(data.verses.len());
            let now = Instant::now();
            process_verses(
                translation_key,
                &data.verses,
                translation_verses,
                highlight_words,
                wip_token_counts,
                proximities,
            );
            info!(
                "Processed {} verses in {}ms",
                data.verses.len(),
                now.elapsed().as_millis()
            );
        }
    }

    info!("Total verses loaded (all translations): {}", total_docs);

    Ok(())
}

/// Build and return a reverse index, fst bytes, and vector of highlight words
#[inline]
fn build_reverse_index(
    highlight_words: &BTreeSet<String>,
    wip_token_counts: &WipTokenCountsMap,
) -> (Vec<ReverseIndexEntryBytes>, Vec<u8>, Vec<String>) {
    let mut build = MapBuilder::memory();
    let mut reverse_index = Vec::with_capacity(wip_token_counts.len());
    let highlight_words: Vec<_> = highlight_words.iter().cloned().collect();

    for (i, (token, entries)) in wip_token_counts.iter().enumerate() {
        build.insert(token.clone(), i as u64).unwrap();

        let mut counts_map_builder = MapBuilder::memory();
        let mut counts_map_data = Vec::new();
        let mut highlights_map_builder = MapBuilder::memory();
        let mut highlights_map_data = Vec::new();

        for (i, (key, vs)) in entries.iter().enumerate() {
            let counts_bytes: Vec<u8> = vs
                .counts
                .iter()
                .flat_map(|c| {
                    (*c as u64)
                        .to_be_bytes()
                        .iter()
                        .copied()
                        .collect::<Vec<u8>>()
                })
                .collect();
            counts_map_builder
                .insert(key.to_be_bytes(), i as u64)
                .expect("Could not insert into counts map builder");
            counts_map_data.push(counts_bytes);

            let highlight_index_bytes: Vec<u8> = vs
                .highlights
                .iter()
                .flat_map(|s| {
                    (highlight_words
                        .binary_search(s)
                        .expect("Could not find index for highlight entry")
                        as u64)
                        .to_be_bytes()
                        .iter()
                        .copied()
                        .collect::<Vec<u8>>()
                })
                .collect();
            highlights_map_builder
                .insert(key.to_be_bytes(), i as u64)
                .expect("Could not insert into highlights map builder");
            highlights_map_data.push(highlight_index_bytes);
        }

        reverse_index.push(ReverseIndexEntryBytes {
            counts_map_bytes: counts_map_builder
                .into_inner()
                .expect("Could not construct counts map bytes"),
            counts_map_data,
            highlights_map_bytes: highlights_map_builder
                .into_inner()
                .expect("Could not construct highlight map bytes"),
            highlights_map_data,
        });
    }

    let fst_bytes = build.into_inner().expect("Could not flush bytes for FST");
    info!("FST compiled: {} bytes", fst_bytes.len());
    info!("Stored {} words for highlighting", highlight_words.len());

    (reverse_index, fst_bytes, highlight_words)
}

#[inline]
fn build_proximity_fst_bytes(
    wip_proximities: &WipProximitiesMap,
    wip_token_counts: &WipTokenCountsMap,
) -> Result<Vec<u8>> {
    let ordered_tokens: Vec<_> = wip_token_counts.keys().cloned().collect();
    let mut proximities_build = MapBuilder::memory();

    for (tidx, m1) in wip_proximities {
        for (vkey, m2) in m1 {
            for (w1, m3) in m2 {
                let w1i = ordered_tokens
                    .binary_search(w1)
                    .expect("Could not find index for token for proximity map")
                    as u16;
                for (w2, p) in m3 {
                    let w2i = ordered_tokens
                        .binary_search(w2)
                        .expect("Could not find index for token for proximity map")
                        as u16;
                    proximities_build
                        .insert(proximity_bytes_key(*tidx as u8, vkey, w1i, w2i), *p)
                        .expect("Could not insert into proximities build");
                }
            }
        }
    }

    let proximities = proximities_build
        .into_inner()
        .context("Could not build proximities map bytes")?;

    Ok(proximities)
}

#[inline]
fn build_translation_verses_bytes(
    translation_verses: &TranslationVerses,
) -> Result<(Vec<u8>, Vec<String>)> {
    let mut strings = Vec::new();
    let mut build = MapBuilder::memory();

    for (tidx, verses) in translation_verses.iter() {
        for (verse_key, text) in verses {
            let key = translation_verses_bytes_key(*tidx as u8, verse_key);
            build
                .insert(key, strings.len() as u64)
                .context("Could not insert into translation verses map builder")?;
            strings.push(text.clone());
        }
    }

    let bytes = build
        .into_inner()
        .context("Could not build translation verses fst bytes")?;

    Ok((bytes, strings))
}

/// Creates and returns a search index
pub fn get_index() -> VersearchIndex {
    let start = Instant::now();

    let mut wip_token_counts = BTreeMap::new();
    let mut wip_proximities = BTreeMap::new();
    let mut translation_verses: TranslationVerses = BTreeMap::new();
    let mut highlight_words = BTreeSet::new();

    load_data(
        &mut translation_verses,
        &mut highlight_words,
        &mut wip_token_counts,
        &mut wip_proximities,
    )
    .expect("Could not load data from disk");

    let now = Instant::now();

    let (reverse_index_bytes, fst_bytes, highlight_words) =
        build_reverse_index(&highlight_words, &wip_token_counts);

    info!("Indexed data {}ms", now.elapsed().as_millis());

    let now = Instant::now();

    let proximities_bytes = build_proximity_fst_bytes(&wip_proximities, &wip_token_counts)
        .expect("Could not build proximities map");

    info!(
        "Proximities FST compiled: {} tokens, {} bytes in {}ms",
        wip_token_counts.len(),
        proximities_bytes.len(),
        now.elapsed().as_millis()
    );

    let (translation_verses_bytes, translation_verses_strings) =
        build_translation_verses_bytes(&translation_verses)
            .expect("Could not construct translation verses fst map");

    translation_verses_bytes.len();
    translation_verses_strings.len();

    info!("get_index done in {}ms", start.elapsed().as_millis());

    VersearchIndex::new(
        fst_bytes,
        reverse_index_bytes,
        proximities_bytes,
        highlight_words,
        translation_verses_bytes,
        translation_verses_strings,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        assert_eq!(
            tokenize("hello, world!"),
            vec![
                Tokenized {
                    source: "hello".to_string(),
                    token: "HELLO".to_string()
                },
                Tokenized {
                    source: "world".to_string(),
                    token: "WORLD".to_string()
                }
            ]
        );
        assert_eq!(
            tokenize("It's all good in the neighborhood which is... good"),
            vec![
                Tokenized {
                    source: "It's".to_string(),
                    token: "ITS".to_string()
                },
                Tokenized {
                    source: "all".to_string(),
                    token: "ALL".to_string(),
                },
                Tokenized {
                    source: "good".to_string(),
                    token: "GOOD".to_string()
                },
                Tokenized {
                    source: "in".to_string(),
                    token: "IN".to_string()
                },
                Tokenized {
                    source: "the".to_string(),
                    token: "THE".to_string()
                },
                Tokenized {
                    source: "neighborhood".to_string(),
                    token: "NEIGHBORHOOD".to_string()
                },
                Tokenized {
                    source: "which".to_string(),
                    token: "WHICH".to_string()
                },
                Tokenized {
                    source: "is".to_string(),
                    token: "IS".to_string()
                },
                Tokenized {
                    source: "good".to_string(),
                    token: "GOOD".to_string()
                },
            ]
        );
    }
}
