use anyhow::anyhow;
use catalog::{Catalog, CatalogFragment};
use ngdp::{
    casc::{idx::Key, CASC},
    listfile::{parse_listfile, ListFile},
    tact::{
        cdn::CDNClient,
        config::{parse_build_config, parse_cdn_config},
        keys::TactKeys,
        root::{parse_root, ContentFlags, LocaleFlags, Root},
    },
    util::parse_hex_bytes,
};
use ribbit::{cdns, versions, Server};
use serde::Deserialize;
use std::{fs::read_to_string, path::PathBuf, str::FromStr};

mod catalog;
mod install;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    wow_path: String,
    tactkeys_path: Option<String>,
    listfile_path: String,
    cdn_override: Option<String>,
}

fn main() -> Result<(), anyhow::Error> {
    let config = read_to_string("config.toml")?;
    let config: Config = toml::from_str(&config)?;
    dbg!(&config);

    match std::env::args().nth(1).as_deref() {
        Some("install") => install::install(&config),
        Some("catalog") => catalog(&config),
        _ => do_stuff(&config),
    }
}

struct State {
    casc: CASC,
    root: Root,
    listfile: ListFile,
}

impl State {
    fn read_file(&self, path: &str) -> Result<Vec<u8>, anyhow::Error> {
        let file_id = self
            .listfile
            .get_id(path)
            .ok_or_else(|| anyhow!("couldn't find file_id for {}", path))?;
        let record = self
            .root
            .lookup_by_fileid_and_flags(file_id, ContentFlags::empty(), LocaleFlags::EN_GB)
            .ok_or_else(|| anyhow!("couldn't find record for file_id: {}", file_id))?;
        self.casc.read_by_ckey(&record.content_key)
    }
}

fn do_stuff(config: &Config) -> Result<(), anyhow::Error> {
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

    let cdncache = CDNClient::new(cdns.clone(), config.cdn_override.clone());

    let build_config_text = cdncache.read_config(&version.build_config)?.read_string()?;
    let build_config = parse_build_config(&build_config_text);
    dbg!(&build_config);

    let state = {
        let mut casc = CASC::new(&config.wow_path, &build_config)?;

        let root = {
            let ckey = Key::from_hex(build_config.root);
            let file = casc.read_by_ckey(&ckey)?;
            parse_root(&file).ok_or_else(|| anyhow!("couldn't parse root"))?
        };

        let listfile = {
            let content = std::fs::read_to_string(&config.listfile_path)?;
            parse_listfile(&content)?
        };

        populate_tact_keys_file(&config, &mut casc.tact_keys)?;

        State {
            casc,
            root,
            listfile,
        }
    };

    // Quick test case for decryption
    // let blp = state.read_file("interface/icons/inv_tigermount.blp")?;
    // assert_eq!(5268, blp.len());

    // let vo = state.read_file("sound/creature/thrall/vo_825_thrall_09_m.ogg")?;
    // assert_eq!(47267, vo.len());

    // let (db, _def) = read_db2(&state, "dbfilesclient/Achievement.db2")?;
    // dbg!(db.get_record(12002));

    let mut files: Vec<&str> = state
        .listfile
        .iter()
        .map(|(name, _id)| name.as_str())
        .filter(|name| name.ends_with(".blp"))
        .collect();
    files.sort();

    // files = vec!["world/expansion07/doodads/nazjatar/8nzj_waterwall_custom_01.m2"];

    files
        .into_iter()
        .filter_map(|f| {
            println!("Loading file: {}. ", f);
            match state.read_file(f) {
                Ok(data) => Some(data),
                Err(e) => {
                    println!("Error loading file: {}, skipping...", e);
                    None
                }
            }
        })
        // .take(30)
        .for_each(|data| {
            if data[0..4] == [0, 0, 0, 0] {
                println!("File looks still encrypted, skipping...");
                return;
            }

            // dbg!(_res);
        });

    // for (_key, entry) in indexes.iter_all_entries() {
    //     let data_file = data_path.join(format!("data.{:03}", entry.archive_index));
    //     let mut buf = vec![0; entry.size as usize];

    //     let mut file = File::open(data_file)?;
    //     file.seek(SeekFrom::Start(entry.offset as u64))?;
    //     file.read_exact(&mut buf)?;

    //     let _data = parse_data(&buf).expect("parsing data").1;
    // }

    Ok(())
}

pub fn populate_tact_keys_file(
    config: &Config,
    tact_keys: &mut TactKeys,
) -> Result<(), anyhow::Error> {
    if let Some(tactkeys_path) = &config.tactkeys_path {
        let path = PathBuf::from_str(tactkeys_path).unwrap();
        let keys = read_to_string(path.join("WoW.txt"))?;
        for line in keys.lines() {
            let (name, key) = match line.split_once(' ') {
                Some(v) => v,
                None => continue,
            };

            let name = parse_hex_bytes::<8>(name);
            let key = parse_hex_bytes::<16>(key);

            match (name, key) {
                (Some(mut name), Some(key)) => {
                    name.reverse();
                    tact_keys.add_key(name, key)
                }
                (_, _) => continue,
            }
        }
    }

    Ok(())
}

fn catalog(config: &Config) -> Result<(), anyhow::Error> {
    let res = versions(Server::EU, "catalogs")?;
    dbg!(&res);

    let version = res
        .iter()
        .max_by_key(|v| v.versions_name.parse().unwrap_or(0))
        .ok_or_else(|| anyhow!("couldn't find a single version"))?;
    dbg!(&version);

    let res = cdns(Server::EU, "catalogs")?;
    let cdns = res
        .iter()
        .find(|v| v.name == "eu")
        .ok_or_else(|| anyhow!("couldn't find eu cdns"))?;
    dbg!(&cdns);

    let cdncache = CDNClient::new(cdns.clone(), config.cdn_override.clone());

    let build_config_text = cdncache.read_config(&version.build_config)?.read_string()?;
    let build_config = parse_build_config(&build_config_text);
    dbg!(&build_config);

    let cdn_config_text = cdncache.read_config(&version.cdn_config)?.read_string()?;
    let cdn_config = parse_cdn_config(&cdn_config_text);
    dbg!(&cdn_config);

    let mut tact_keys = TactKeys::default();
    populate_tact_keys_file(&config, &mut tact_keys)?;

    // let index_raw = cdncache.read_index(cdn_config.file_index.hash, cdn_config.file_index.size)?;
    // let index = parse_index(&index_raw).ok_or_else(|| anyhow!("couldn't parse index"))?;
    // dbg!(index);

    let catalog_text = cdncache.read_data(build_config.root)?.read_string()?;
    let catalog: Catalog = serde_json::from_str(&catalog_text)?;
    // dbg!(&catalog);

    for fragment in &catalog.fragments {
        dbg!(fragment);
        if fragment.encrypted_hash.is_some() {
            println!(
                "Catalog fragment '{}' is encrypted, skipping...",
                fragment.name
            );
            continue;
        }

        let fragment_text = cdncache.read_data(&fragment.hash)?.read_string()?;
        println!("{}", fragment_text);
        println!();
        let fragment: CatalogFragment = serde_json::from_str(&fragment_text)?;
        dbg!(&fragment);
    }

    Ok(())
}
