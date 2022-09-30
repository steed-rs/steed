use std::{collections::HashMap, iter::FromIterator};

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
    pub root: &'a str,
    pub install: Option<EncodedPair<HashSize<'a>>>,
    pub download: Option<EncodedPair<HashSize<'a>>>,
    pub size: Option<EncodedPair<HashSize<'a>>>,
    /// Optional
    pub build_partial_priority: Vec<HashSize<'a>>,
    /// Optional
    pub partial_priority: Vec<&'a str>,
    /// Not really understood, always zero if present, present if partial_priority is present
    pub partial_priority_size: Option<&'a str>,
    pub encoding: Option<EncodedPair<HashSize<'a>>>,
    pub patch: Option<HashSize<'a>>,
    pub patch_config: Option<&'a str>,
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

pub fn parse_build_config(input: &str) -> BuildConfig {
    let rough = parse_rough(input);

    let build_partial_priority = rough
        .get("build-partial-priority")
        .map_or_else(Vec::new, |v| {
            v.split_ascii_whitespace()
                .map(|e| {
                    e.split_once(':')
                        .expect("no colon in build_partial_priority value")
                })
                .map(|(h, s)| parse_hashsize(Some(h), Some(s)).unwrap())
                .collect()
        });

    let partial_priority = rough
        .get("size")
        .map_or_else(Vec::new, |v| Vec::from_iter(v.split_ascii_whitespace()));

    BuildConfig {
        root: rough["root"],
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
        patch: parse_hashsize(
            rough.get("patch").copied(),
            rough.get("patch-size").copied(),
        ),
        patch_config: rough.get("patch-config").copied(),
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
    }
}

#[derive(Debug)]
pub struct CDNConfig<'a> {
    pub archives: Vec<&'a str>,
    pub archives_index_size: Vec<usize>,
    pub archive_group: Option<&'a str>,
    pub file_index: HashSize<'a>,
    pub patch_archives: Vec<&'a str>,
    pub patch_archives_index_size: Vec<usize>,
    pub patch_archive_group: Option<&'a str>,
    pub patch_file_index: Option<HashSize<'a>>,
    pub builds: Vec<&'a str>,
}

pub fn parse_cdn_config(input: &str) -> CDNConfig {
    let rough = parse_rough(input);

    // TODO: Assumptions about list lengths are made here
    let archives = rough
        .get("archives")
        .iter()
        .flat_map(|v| v.split(' '))
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
        archive_group: rough.get("archive-group").copied(),
        file_index: parse_hashsize(
            rough.get("file-index").copied(),
            rough.get("file-index-size").copied(),
        )
        .expect("missing file-index from cdn-config"),
        patch_archives,
        patch_archives_index_size,
        patch_archive_group: rough.get("patch-archive-group").copied(),
        patch_file_index: parse_hashsize(
            rough.get("patch-file-index").copied(),
            rough.get("patch-file-index-size").copied(),
        ),
        builds,
    }
}

#[derive(Debug)]
pub struct EncodedPair<T> {
    pub decoded: T,
    pub encoded: Option<T>,
}

#[derive(Debug, Clone, Copy)]
pub struct HashSize<'a> {
    pub hash: &'a str,
    pub size: usize,
}

fn parse_pair_hashkey<'a>(
    hash: Option<&'a str>,
    size: Option<&'a str>,
) -> Option<EncodedPair<HashSize<'a>>> {
    let (hash, size) = (hash?, size?);
    let (decoded_hash, encoded_hash) = hash.split_once(' ').unwrap_or((hash, ""));
    let (decoded_size, encoded_size) = size.split_once(' ').unwrap_or((size, ""));
    Some(EncodedPair {
        decoded: parse_hashsize(Some(decoded_hash), Some(decoded_size))?,
        encoded: match (encoded_hash, encoded_size) {
            ("", _) => None,
            (_, "") => None,
            (hash, size) => parse_hashsize(Some(hash), Some(size)),
        },
    })
}

fn parse_hashsize<'a>(hash: Option<&'a str>, size: Option<&'a str>) -> Option<HashSize<'a>> {
    let (hash, size) = (hash?, size?);
    Some(HashSize {
        hash,
        size: size.parse().expect("hash size was not a number"),
    })
}