use anyhow::{anyhow, Context};
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressStyle};
use ngdp::{
    casc::{
        blte::{compute_md5, decode_blte},
        idx::{self, Indexes, Key},
        shmem::Shmem,
        FileHeader,
    },
    tact::{
        cdn::{CDNClient, CDNReader},
        config::{parse_build_config, parse_cdn_config},
        download::{self, parse_download_manifest},
        encoding::parse_encoding,
        index::parse_index,
        install::parse_install_manifest,
        keys::TactKeys,
    },
    util::{format_hex_bytes, parse_hex_bytes},
};
use ribbit::{cdns, versions, Server};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::Config;

const MAIN_BAR_STYLE: &str = "{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})";
const SUB_BAR_STYLE: &str = "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})";
const COUNT_BAR_STYLE: &str =
    "{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({per_sec}, {eta})";

#[derive(Serialize, Deserialize, Debug)]
struct InstallState {
    install_tags: HashSet<String>,
    download_tags: HashSet<String>,
    installed_files: HashSet<[u8; 16]>,
    // TODO: Include version?
}

pub fn install(config: &Config) -> Result<(), anyhow::Error> {
    let dir = std::env::args().nth(2).unwrap();
    let dir = PathBuf::from(dir);
    println!("{}", dir.display());

    println!("Attempting to load CASC state...");
    let mut builder = match CASCBuilder::load(&dir) {
        Ok(builder) => builder,
        Err(e) => {
            eprintln!("No or invalid CASC state, starting fresh. ({})", e);
            CASCBuilder::new(&dir)
        }
    };

    println!("Rebuilding unused space structure...");
    builder.shmem.rebuild_unused_from_index(&builder.indexes);

    println!("Attempting to load existing install progress...");
    let mut state = match load_state(&dir) {
        Ok(state) => state,
        Err(e) => {
            println!("No or invalid install state, starting fresh. ({})", e);
            // TODO: Parse tags from args
            InstallState {
                install_tags: HashSet::from_iter(
                    // TODO: speech vs text, what's the diff?
                    vec!["Windows", "x86_64", "enUS", "EU", "speech"]
                        .into_iter()
                        .map(|s| s.to_string()),
                ),
                download_tags: HashSet::from_iter(
                    vec!["Windows", "x86_64", "enUS", "EU", "speech"]
                        .into_iter()
                        .map(|s| s.to_string()),
                ),
                installed_files: HashSet::new(),
            }
        }
    };

    let res = install_inner(config, &dir, &mut builder, &mut state);
    match res {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("Error while installing: {}", e);

            println!("Saving CASC state...");
            builder.write()?;

            println!("Saving installation progress...");
            let state_data = bincode::serialize(&state)?;
            std::fs::write(dir.join(INSTALL_STATE_NAME), &state_data)?;

            Err(e)
        }
    }
}

const INSTALL_STATE_NAME: &str = ".steed-install-state";

fn load_state(dir: &Path) -> Result<InstallState, anyhow::Error> {
    let content = std::fs::read(dir.join(INSTALL_STATE_NAME))?;
    let state = bincode::deserialize(&content)?;
    Ok(state)
}

fn install_inner(
    config: &Config,
    dir: &Path,
    builder: &mut CASCBuilder,
    state: &mut InstallState,
) -> Result<(), anyhow::Error> {
    let mb = MultiProgress::new();

    // TODO: We're being really naive with memory, keeping this ~256M buffer life this long
    let mut buf: Vec<u8> = vec![];

    let retail_dir = dir.join("_retail_");
    let data_dir = dir.join("Data").join("data");

    std::fs::create_dir_all(&retail_dir)?;
    std::fs::create_dir_all(&data_dir)?;

    let res = versions(Server::EU, "wow")?;
    let version = res
        .iter()
        .find(|v| v.region == "eu")
        .ok_or_else(|| anyhow!("couldn't find eu version"))?;
    dbg!(&version);

    let res = cdns(Server::EU, "wow")?;
    let cdns = res
        .iter()
        .find(|v| v.name == "eu")
        .ok_or_else(|| anyhow!("couldn't find eu cdns"))?;
    dbg!(&cdns);

    let mut cdn = CDNClient::new(cdns.clone(), config.cdn_override.clone());

    let build_config_text = builder.read_config(&cdn, &version.build_config)?;
    let build_config = parse_build_config(&build_config_text);
    // dbg!(&build_config);

    let cdn_config_text = builder.read_config(&cdn, &version.cdn_config)?;
    let cdn_config = parse_cdn_config(&cdn_config_text);
    // dbg!(&cdn_config);

    println!("Ranking CDN servers...");
    cdn.rank_servers(cdn_config.archives[0])?;

    let tact_keys = TactKeys::default();
    // populate_tact_keys_file(&config, &mut tact_keys)?;

    let encoding = {
        let encoding_hs = build_config
            .encoding
            .as_ref()
            .ok_or_else(|| anyhow!("build config had no encoding field"))?
            .encoded
            .ok_or_else(|| anyhow!("encoded hash for encoding file not found, can't progress"))?;
        let encoding_data = cdn
            .read_data(encoding_hs.hash)?
            .read_vec(encoding_hs.size)?;
        let encoding_data = decode_blte(&tact_keys, &encoding_data)?;
        parse_encoding(&encoding_data).context("parsing encoding")?
    };

    let bar = mb.add(ProgressBar::new(cdn_config.archives.len() as u64));
    bar.set_style(
        ProgressStyle::with_template(COUNT_BAR_STYLE)
            .unwrap()
            .progress_chars("#>-"),
    );
    bar.set_message("Building file index");

    let mut archived_files = HashMap::new();
    let mut archive_sizes = HashMap::new();
    for (archive, index_size) in cdn_config
        .archives
        .iter()
        .zip(cdn_config.archives_index_size)
    {
        let index_data = builder.read_archive_index(&cdn, archive, index_size)?;
        let index = parse_index(&index_data)?;

        let size: u64 = index.entries.values().map(|e| e.size).sum();
        archive_sizes.insert(*archive, size);

        for (key, entry) in index.entries {
            assert!(archived_files.insert(key, (*archive, entry)).is_none());
        }
        bar.inc(1);
    }
    bar.finish();

    let install_manifest_hs = build_config
        .install
        .as_ref()
        .ok_or_else(|| anyhow!("build config had no install field"))?
        .encoded
        .ok_or_else(|| anyhow!("decoded install manifest key not supported"))?;
    let install_manifest_data = cdn
        .read_data(install_manifest_hs.hash)?
        .read_vec(install_manifest_hs.size)?;
    let install_manifest = parse_install_manifest(&tact_keys, &install_manifest_data)?;

    let total_bytes: u64 = install_manifest
        .files_with_tags(&state.install_tags)
        .map(|f| f.size as u64)
        .sum();

    let bar = mb.add(ProgressBar::new(total_bytes));
    bar.set_style(
        ProgressStyle::with_template(MAIN_BAR_STYLE)
            .unwrap()
            .progress_chars("#>-"),
    );

    for file in install_manifest.files_with_tags(&state.install_tags) {
        // TODO: Actual case folding
        let file_name = file.name.to_lowercase().replace('\\', "/");
        bar.set_message(file_name.clone());

        if state.installed_files.contains(&file.key.0) {
            continue;
        }

        || -> Result<(), anyhow::Error> {
            let path = retail_dir.join(PathBuf::from(&file_name));
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let already_installed = || -> Result<bool, anyhow::Error> {
                let mut f = File::open(&path)?;
                let res = read_md5(&mut f)?;
                Ok(res == file.key.0)
            }();
            if already_installed.unwrap_or(false) {
                bar.inc(file.size as u64);
                return Ok(());
            }

            let ce_entry = encoding.lookup_by_ckey(&file.key).ok_or_else(|| {
                anyhow::anyhow!("Couldn't find encoding for ckey: {:?}", &file.key)
            })?;
            let ekey = &ce_entry.ekeys[0];

            let mut reader = if let Some((archive, entry)) = archived_files.get(ekey) {
                cdn.read_data_part(archive, entry.offset as usize, entry.size as usize)?
            } else {
                cdn.read_data(&format!("{:?}", ekey))?
            };
            read_with_bar(&mb, &mut reader, &mut buf, file.size as usize)?;

            let data = decode_blte(&tact_keys, &buf)?;
            std::fs::write(&path, data)?;

            Ok(())
        }()?;

        state.installed_files.insert(file.key.0);
        bar.inc(file.size as u64);
    }
    bar.finish();

    let download_manifest_hs = build_config
        .download
        .ok_or_else(|| anyhow!("build config had no download key"))?
        .encoded
        .ok_or_else(|| anyhow!("decoded download manifest key not supported"))?;
    let download_manifest_data = cdn
        .read_data(download_manifest_hs.hash)?
        .read_vec(download_manifest_hs.size)?;
    let download_manifest = parse_download_manifest(&tact_keys, &download_manifest_data)?;

    // START: Download plan
    let mut total_bytes = 0u64;
    let mut finished_bytes = 0u64;

    let mut by_archive = HashMap::<_, Vec<_>>::new();
    let mut loose = vec![];
    for file in download_manifest.entries_with_tags(&state.download_tags) {
        total_bytes += file.file_size;

        if builder.indexes.lookup(&file.key).is_some() {
            finished_bytes += file.file_size;
            continue;
        }

        if let Some((archive, entry)) = archived_files.get(&file.key) {
            by_archive.entry(*archive).or_default().push((file, entry));
        } else {
            loose.push(file);
        };
    }

    let mut archive_order = Vec::from_iter(by_archive.keys().copied());
    archive_order.sort_by_cached_key(|a| {
        by_archive[a]
            .iter()
            .map(|(f, _e)| f.download_priority as u64)
            .sum::<u64>()
    });
    // END: Download plan

    let bar = mb.add(ProgressBar::new(total_bytes));
    bar.set_style(
        ProgressStyle::with_template(MAIN_BAR_STYLE)
            .unwrap()
            .progress_chars("#>-"),
    );
    bar.inc(finished_bytes);

    let mut allocate_and_write =
        |file: &download::Entry, reader: &mut dyn Read| -> Result<(), anyhow::Error> {
            let total_size = file.file_size as usize + FileHeader::SIZE;

            let slot = builder
                .shmem
                .reserve_bytes(total_size as usize)
                .ok_or_else(|| anyhow!("no more free space in shmem"))?;

            if slot.data_file_missing == 1 {
                // Anything?
            }

            let path = data_dir.join(format!("data.{:03}", slot.data_number));

            let mut f = File::options().create(true).write(true).open(path)?;
            assert!(
                f.metadata()?.len() >= slot.offset as u64,
                "recieved offset is outside file bounds"
            );
            f.seek(SeekFrom::Start(slot.offset as u64))?;

            let header = FileHeader {
                hash: file.key.to_rev(),
                size: total_size as u32,
                _unk: [0, 0],
                checksum_a: 0xdeadbeef,
                checksum_b: 0xdeafbeef,
            };
            header.write_to(slot.data_number, slot.offset, &mut f)?;
            copy_with_bar(&mb, reader, &mut f, file.file_size as usize)?;

            // Adding to index last as index should only contain complete entries
            builder.insert_in_index(
                &file.key,
                idx::Entry {
                    archive_index: slot.data_number,
                    offset: slot.offset,
                    size: total_size as u32,
                },
            );

            Ok(())
        };

    let mut bulk_bandwidth_sum = 0.0f64;
    let mut num_bulk_dls = 0u32;
    let mut wait_time = 0.0f64;
    let mut num_reqs = 0u32;

    for (archive, entries) in archive_order.into_iter().map(|a| (a, &by_archive[a])) {
        let archive_size = archive_sizes[archive];
        let entries_size: u64 = entries.iter().map(|(f, _e)| f.file_size).sum();
        let waste = 1.0 - entries_size as f64 / archive_size as f64;

        let do_parts = {
            let bandwidth = bulk_bandwidth_sum / num_bulk_dls as f64;
            let req_overhead = wait_time / num_reqs as f64;
            let archive_est = req_overhead + 256_000_000.0 / bandwidth;
            let parts_est = entries.len() as f64 * req_overhead + entries_size as f64 / bandwidth;
            bar.set_message(format!(
                "archive {} ({} entries, {:.02}% waste, bw {}/s, {} req/s, archive est {}, parts est {})",
                archive,
                entries.len(),
                waste * 100.0,
                HumanBytes(bandwidth as u64),
                indicatif::HumanFloatCount(1.0 / req_overhead.max(0.0)),
                indicatif::HumanDuration(Duration::from_secs_f64(archive_est.max(0.0))),
                indicatif::HumanDuration(Duration::from_secs_f64(parts_est.max(0.0))),
            ));
            parts_est < archive_est
        };

        if do_parts {
            for (file, entry) in entries {
                let start = Instant::now();
                let mut reader =
                    cdn.read_data_part(archive, entry.offset as usize, entry.size as usize)?;

                wait_time += start.elapsed().as_secs_f64();
                num_reqs += 1;

                allocate_and_write(file, &mut reader)?;

                bar.inc(file.file_size);
            }
        } else {
            let start = Instant::now();
            let mut reader = cdn.read_data(archive)?;

            wait_time += start.elapsed().as_secs_f64();
            num_reqs += 1;

            read_with_bar(&mb, &mut reader, &mut buf, archive_size as usize)?;

            bulk_bandwidth_sum += reader.avg_bandwidth();
            num_bulk_dls += 1;

            for (file, entry) in entries {
                let data = &buf[entry.offset as usize..][..entry.size as usize];
                allocate_and_write(file, &mut Cursor::new(data))?;

                bar.inc(file.file_size);
            }
        }
    }

    for file in loose {
        let mut reader = cdn.read_data(&format!("{:?}", &file.key))?;
        allocate_and_write(file, &mut reader)?;
        bar.inc(file.file_size);
    }
    bar.finish();

    println!("Saving CASC state...");
    builder.write()?;

    // TODO: Generate .build.info

    Ok(())
}

fn read_md5(r: &mut impl Read) -> Result<[u8; 16], anyhow::Error> {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    std::io::copy(r, &mut hasher)?;
    let res = hasher.finalize();
    Ok(res.into())
}

fn read_with_bar(
    mb: &MultiProgress,
    r: &mut impl Read,
    buf: &mut Vec<u8>,
    expected_size: usize,
) -> Result<(), anyhow::Error> {
    buf.clear();
    buf.reserve(expected_size);

    if expected_size > 1_000_000 {
        let bar = mb.add(ProgressBar::new(expected_size as u64));
        bar.set_style(
            ProgressStyle::with_template(SUB_BAR_STYLE)
                .unwrap()
                .progress_chars("#>-"),
        );

        let mut r = bar.wrap_read(r);
        r.read_to_end(buf)?;
        bar.finish_and_clear();
    } else {
        r.read_to_end(buf)?;
    }
    Ok(())
}

fn copy_with_bar(
    mb: &MultiProgress,
    mut r: impl Read,
    file: &mut File,
    expected_size: usize,
) -> Result<(), anyhow::Error> {
    if expected_size > 1_000_000 {
        let bar = mb.add(ProgressBar::new(expected_size as u64));
        bar.set_style(
            ProgressStyle::with_template(SUB_BAR_STYLE)
                .unwrap()
                .progress_chars("#>-"),
        );

        let mut r = bar.wrap_read(r);
        std::io::copy(&mut r, file)?;
        bar.finish_and_clear();
    } else {
        std::io::copy(&mut r, file)?;
    }
    Ok(())
}

pub struct CASCBuilder {
    root: PathBuf,
    shmem: Shmem,
    indexes: Indexes,
    index_changed: [bool; 16], // Really can't be bothere to make it a bitset
}

impl CASCBuilder {
    pub fn new(root: &Path) -> Self {
        CASCBuilder {
            root: root.to_owned(),
            // TODO: I'm just copying what WoW does, is that correct?
            shmem: Shmem::new("Global\\../Data/data"),
            indexes: Indexes::default(),
            index_changed: [false; 16],
        }
    }

    pub fn load(root: &Path) -> Result<Self, anyhow::Error> {
        let data_path = root.join("Data").join("data");

        let shmem_data = std::fs::read(data_path.join("shmem"))?;
        let shmem = Shmem::parse(&shmem_data)?;

        let indexes = Indexes::read(&data_path, &shmem)?;

        Ok(CASCBuilder {
            root: root.to_owned(),
            shmem,
            indexes,
            index_changed: [false; 16],
        })
    }

    pub fn write(&mut self) -> Result<(), anyhow::Error> {
        let data_dir = self.root.join("Data").join("data");

        for (idx, has_changed) in self.index_changed.iter().copied().enumerate() {
            if has_changed {
                // This could technically overflow, but for ordinary usage it'll never happen
                self.shmem.index_versions[idx] += 1;
            }
        }

        // TODO: Delete old versions
        self.indexes.write(self.shmem.index_versions, &data_dir)?;

        let mut buf = Vec::with_capacity(16 * 1024);
        self.shmem.write(&mut buf)?;
        std::fs::write(data_dir.join("shmem"), buf)?;

        Ok(())
    }

    pub fn read_config(&self, cdn: &CDNClient, key: &str) -> Result<String, anyhow::Error> {
        let path = self
            .root
            .join("Data")
            .join("config")
            .join(&key[0..2])
            .join(&key[2..4])
            .join(key);
        let res = self.try_read(
            &path,
            0,
            || cdn.read_config(key),
            |data| {
                let expected_hash = parse_hex_bytes::<16>(key).expect("wrong key length");
                let hash = compute_md5(data);
                if hash != expected_hash {
                    anyhow::bail!(
                        "config hash not correct! expected: {:02x?}, calculated: {:02x?}",
                        key,
                        format_hex_bytes(&hash)
                    );
                }
                Ok(())
            },
        )?;

        String::from_utf8(res).map_err(|e| e.into())
    }

    pub fn read_archive_index(
        &self,
        cdn: &CDNClient,
        key: &str,
        expected_size: usize,
    ) -> Result<Vec<u8>, anyhow::Error> {
        let key = format!("{}.index", key);
        let path = self.root.join("Data").join("indices").join(&key);
        // TODO: Verify
        self.try_read(&path, expected_size, || cdn.read_data(&key), |_data| Ok(()))
    }

    fn try_read(
        &self,
        path: &Path,
        expected_size: usize,
        get_reader: impl FnOnce() -> Result<CDNReader, anyhow::Error>,
        verify: impl FnOnce(&[u8]) -> Result<(), anyhow::Error>,
    ) -> Result<Vec<u8>, anyhow::Error> {
        // If any error occurs, redownload the config
        if let Ok(res) = std::fs::read(path) {
            verify(&res)?;
            return Ok(res);
        };

        let mut reader = get_reader()?;
        let data = reader.read_vec(expected_size)?;

        let res = || -> Result<(), anyhow::Error> {
            let parent = path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("read path had no parent directory"))?;
            std::fs::create_dir_all(parent)?;
            std::fs::write(&path, &data)?;
            Ok(())
        }();

        match res {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error saving copy of file {}: {}", path.display(), e);
            }
        }

        Ok(data)
    }

    pub fn insert_in_index(&mut self, k: &Key, entry: idx::Entry) {
        // TODO: Should we error on duplicate entry?
        let (idx, _) = self.indexes.insert(k, entry);
        self.index_changed[idx] = true;
    }
}
