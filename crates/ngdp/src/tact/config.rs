use std::{collections::HashMap, fmt::Debug, iter::FromIterator};

use super::{ContentKey, EncodingKey};

fn parse_rough(config: &str) -> HashMap<&str, &str> {
    let mut res = HashMap::new();
    for line in config.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        let (key, value) = line
            .split_once('=')
            .expect("key-value line had no equals sign");

        res.insert(key.trim(), value.trim());
    }
    res
}

#[derive(Debug)]
pub struct BuildConfig<'a> {
    // TODO: Type the keys?
    pub root: ContentKey,
    pub install: Option<EncodedPair>,
    pub download: Option<EncodedPair>,
    pub size: Option<EncodedPair>,
    /// Optional
    pub build_partial_priority: Vec<HashSize<ContentKey>>,
    /// Optional
    pub partial_priority: Vec<ContentKey>,
    /// Not really understood, always zero if present, present if partial_priority is present
    pub partial_priority_size: Option<&'a str>,
    pub encoding: Option<EncodedPair>,
    pub patch: Option<HashSize<EncodingKey>>,
    pub patch_config: Option<ContentKey>,
    pub build_attributes: Option<&'a str>,
    pub build_branch: Option<&'a str>,
    pub build_comments: Option<&'a str>,
    pub build_creator: Option<&'a str>,
    pub build_fixed_hash: Option<&'a str>,
    pub build_replay_hash: Option<&'a str>,
    pub build_name: Option<&'a str>,
    pub build_playbuild_installer: Option<&'a str>,
    pub build_product: Option<&'a str>,
    pub build_t1_manifest_version: Option<&'a str>,
    pub build_uid: Option<&'a str>,
}

pub fn parse_build_config(input: &str) -> Result<BuildConfig, anyhow::Error> {
    let rough = parse_rough(input);

    let build_partial_priority = rough
        .get("build-partial-priority")
        .map_or_else(Vec::new, |v| {
            v.split_ascii_whitespace()
                .map(|e| {
                    e.split_once(':')
                        .expect("no colon in build_partial_priority value")
                })
                .map(|(h, s)| parse_hashsize_content(h, s).unwrap())
                .collect()
        });

    let partial_priority = rough
        .get("partial-priority")
        .iter()
        .flat_map(|v| v.split_ascii_whitespace())
        .map(ContentKey::parse)
        .collect::<Result<_, _>>()?;

    Ok(BuildConfig {
        root: ContentKey::parse(rough["root"])?,
        install: parse_pair_hashkey(
            rough.get("install").copied(),
            rough.get("install-size").copied(),
        ),
        download: parse_pair_hashkey(
            rough.get("download").copied(),
            rough.get("download-size").copied(),
        ),
        size: parse_pair_hashkey(rough.get("size").copied(), rough.get("size-size").copied()),
        build_partial_priority,
        partial_priority,
        partial_priority_size: rough.get("partial-priority-size").copied(),
        encoding: parse_pair_hashkey(
            rough.get("encoding").copied(),
            rough.get("encoding-size").copied(),
        ),
        patch: parse_hashsize_encoding(
            rough.get("patch").copied(),
            rough.get("patch-size").copied(),
        ),
        patch_config: rough
            .get("patch-config")
            .map(|s| ContentKey::parse(s))
            .transpose()?,
        build_attributes: rough.get("build-attributes").copied(),
        build_branch: rough.get("build-branch").copied(),
        build_comments: rough.get("build-comments").copied(),
        build_creator: rough.get("build-creator").copied(),
        build_fixed_hash: rough.get("build-fixed-hash").copied(),
        build_replay_hash: rough.get("build_replay_hash").copied(),
        build_name: rough.get("build-name").copied(),
        build_playbuild_installer: rough.get("build-playbuild-installer").copied(),
        build_product: rough.get("build-product").copied(),
        build_t1_manifest_version: rough.get("build-t1-manifest-version").copied(),
        build_uid: rough.get("build-uid").copied(),
    })
}

#[derive(Debug)]
pub struct CDNConfig<'a> {
    pub archives: Vec<EncodingKey>,
    pub archives_index_size: Vec<usize>,
    pub archive_group: Option<EncodingKey>,
    pub file_index: HashSize<EncodingKey>,
    pub patch_archives: Vec<EncodingKey>,
    pub patch_archives_index_size: Vec<usize>,
    pub patch_archive_group: Option<EncodingKey>,
    pub patch_file_index: Option<HashSize<EncodingKey>>,
    pub builds: Vec<&'a str>,
}

pub fn parse_cdn_config(input: &str) -> CDNConfig {
    let rough = parse_rough(input);

    // TODO: Assumptions about list lengths are made here
    let archives = rough
        .get("archives")
        .iter()
        .flat_map(|v| v.split(' '))
        .map(|s| EncodingKey::parse(s).unwrap())
        .collect();

    let archives_index_size = rough
        .get("archives-index-size")
        .iter()
        .flat_map(|v| v.split(' '))
        .map(|v| v.parse().expect("archive index size was not integer"))
        .collect();

    let patch_archives = rough
        .get("patch-archives")
        .iter()
        .flat_map(|v| v.split(' '))
        .map(|s| EncodingKey::parse(s).unwrap())
        .collect();

    let patch_archives_index_size = rough
        .get("patch-archives-index-size")
        .iter()
        .flat_map(|v| v.split(' '))
        .map(|v| v.parse().expect("patch archive index size was not integer"))
        .collect();

    let builds = rough
        .get("builds")
        .map_or_else(Vec::new, |v| Vec::from_iter(v.split_ascii_whitespace()));

    CDNConfig {
        archives,
        archives_index_size,
        archive_group: rough
            .get("archive-group")
            .and_then(|s| EncodingKey::parse(s).ok()),
        file_index: parse_hashsize_encoding(
            rough.get("file-index").copied(),
            rough.get("file-index-size").copied(),
        )
        .expect("missing file-index from cdn-config"),
        patch_archives,
        patch_archives_index_size,
        patch_archive_group: rough
            .get("patch-archive-group")
            .and_then(|s| EncodingKey::parse(s).ok()),
        patch_file_index: parse_hashsize_encoding(
            rough.get("patch-file-index").copied(),
            rough.get("patch-file-index-size").copied(),
        ),
        builds,
    }
}

#[derive(Debug)]
pub struct EncodedPair {
    pub decoded: HashSize<ContentKey>,
    pub encoded: Option<HashSize<EncodingKey>>,
}

#[derive(Debug, Clone, Copy)]
pub struct HashSize<T> {
    pub hash: T,
    pub size: usize,
}

fn parse_pair_hashkey(hash: Option<&str>, size: Option<&str>) -> Option<EncodedPair> {
    let (hash, size) = (hash?, size?);
    let (decoded_hash, encoded_hash) = hash.split_once(' ').unwrap_or((hash, ""));
    let (decoded_size, encoded_size) = size.split_once(' ').unwrap_or((size, ""));
    Some(EncodedPair {
        decoded: parse_hashsize_content(decoded_hash, decoded_size)?,
        encoded: match (encoded_hash, encoded_size) {
            ("", _) => None,
            (_, "") => None,
            (hash, size) => parse_hashsize_encoding(Some(hash), Some(size)),
        },
    })
}

fn parse_hashsize_content(hash: &str, size: &str) -> Option<HashSize<ContentKey>> {
    Some(HashSize {
        hash: ContentKey::parse(hash).expect("hash was not a valid 32 character hex string"),
        size: size.parse().expect("hash size was not a number"),
    })
}

fn parse_hashsize_encoding(
    hash: Option<&str>,
    size: Option<&str>,
) -> Option<HashSize<EncodingKey>> {
    let (hash, size) = (hash?, size?);
    Some(HashSize {
        hash: EncodingKey::parse(hash).expect("hash was not a valid 32 character hex string"),
        size: size.parse().expect("hash size was not a number"),
    })
}
