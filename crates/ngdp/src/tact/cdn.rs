use std::{
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::bail;
use reqwest::{
    blocking::{Client, ClientBuilder, Response},
    header::RANGE,
};
use running_average::RealTimeRunningAverage;

use crate::util::format_hex_bytes;

use super::{ContentKey, EncodingKey};

pub struct CDNClient {
    servers: Vec<String>,
    cdn_path: String,
    cdn_override: Option<String>,
    client: Client,
}

impl CDNClient {
    pub fn new(cdns: ribbit::CDNS, cdn_override: Option<String>) -> CDNClient {
        let mut servers = vec![];
        for server in cdns.servers {
            if let Some(query_pos) = server.find('?') {
                let (server, _) = server.split_at(query_pos);
                servers.push(server.to_string());
            } else {
                servers.push(server);
            }
        }

        CDNClient {
            servers,
            cdn_path: cdns.path,
            cdn_override,
            client: ClientBuilder::new()
                .tcp_keepalive(Duration::from_secs(60))
                .build()
                .unwrap(),
        }
    }

    pub fn rank_servers(&mut self, key: &EncodingKey) -> Result<(), anyhow::Error> {
        let mut buf = vec![0; 16 * 1024 * 1024];
        let mut servers = vec![];

        let path = self.data_path(key);
        for server in &self.servers {
            let url = format!("{}{}", server, path.display());

            let res = || -> Result<(), anyhow::Error> {
                let resp = self.client.get(&url).send()?;
                if resp.status().is_success() {
                    let mut reader = CDNReader::new(resp);

                    let start = Instant::now();
                    reader.read_exact(&mut buf)?;

                    let duration = start.elapsed();
                    servers.push((server.clone(), duration));
                } else {
                    bail!("404 fetching file: {}", url);
                }
                Ok(())
            }();

            match res {
                Ok(_) => {}
                Err(e) => eprintln!("Error ranking server {}: {}", server, e),
            }
        }

        servers.sort_by_key(|(_server, duration)| *duration);
        self.servers = servers.into_iter().map(|v| v.0).collect();
        Ok(())
    }

    pub fn read_config(&self, key: &ContentKey) -> Result<CDNReader, anyhow::Error> {
        let path = self.config_path(key);
        self.read(&path)
    }

    pub fn read_data(&self, key: &EncodingKey) -> Result<CDNReader, anyhow::Error> {
        let path = self.data_path(key);
        self.read(&path)
    }

    pub fn read_index(&self, key: &EncodingKey) -> Result<CDNReader, anyhow::Error> {
        let path = self.index_path(key);
        self.read(&path)
    }

    pub fn read_data_part(
        &self,
        key: &EncodingKey,
        offset: usize,
        size: usize,
    ) -> Result<CDNReader, anyhow::Error> {
        let path = self.data_path(key);
        self.read_part(&path, offset, size)
    }

    fn read(&self, path: &Path) -> Result<CDNReader, anyhow::Error> {
        let mut last_error = anyhow::anyhow!("No CDNs defined");
        for server in self.servers() {
            let url = self.cdn_url(server, path);
            let resp = self.client.get(url).send();
            match resp {
                Ok(resp) if resp.status().is_success() => return Ok(CDNReader::new(resp)),
                _ => {
                    if let Err(e) = resp {
                        last_error = e.into();
                    }
                    // Try next CDN
                    continue;
                }
            }
        }
        Err(last_error)
    }

    fn read_part(
        &self,
        path: &Path,
        offset: usize,
        size: usize,
    ) -> Result<CDNReader, anyhow::Error> {
        for server in self.servers() {
            let url = self.cdn_url(server, path);
            let resp = self
                .client
                .get(url)
                .header(RANGE, format!("bytes={}-{}", offset, offset + size))
                .send()?;
            if resp.status().is_success() {
                return Ok(CDNReader::new(resp));
            } else {
                // Try next cdn
                continue;
            }
        }
        bail!("404 fetching file: {}", path.display())
    }

    fn config_path(&self, key: &ContentKey) -> PathBuf {
        let key = format_hex_bytes(&key.to_inner());
        PathBuf::from(&self.cdn_path)
            .join("config")
            .join(&key[0..2])
            .join(&key[2..4])
            .join(key)
    }

    fn data_path(&self, key: &EncodingKey) -> PathBuf {
        let key = format_hex_bytes(&key.to_inner());
        PathBuf::from(&self.cdn_path)
            .join("data")
            .join(&key[0..2])
            .join(&key[2..4])
            .join(key)
    }

    fn index_path(&self, key: &EncodingKey) -> PathBuf {
        let key = format_hex_bytes(&key.to_inner());
        PathBuf::from(&self.cdn_path)
            .join("data")
            .join(&key[0..2])
            .join(&key[2..4])
            .join(format!("{}.index", key))
    }

    fn servers(&self) -> impl Iterator<Item = &String> {
        self.cdn_override.iter().chain(self.servers.iter())
    }

    fn cdn_url(&self, server: &str, path: &Path) -> String {
        let server = self.cdn_override.as_deref().unwrap_or(server);
        format!("{}{}", server, path.display())
    }
}

pub struct CDNReader {
    resp: Response,
    bandwidth: RealTimeRunningAverage<f32>,
}

impl CDNReader {
    fn new(resp: Response) -> CDNReader {
        CDNReader {
            resp,
            bandwidth: RealTimeRunningAverage::new(Duration::from_secs(10)),
        }
    }

    pub fn avg_bandwidth(&mut self) -> f64 {
        self.bandwidth.measurement().rate()
    }
}

impl CDNReader {
    pub fn read_vec(&mut self, expected_size: usize) -> std::io::Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(expected_size);
        self.read_to_end(&mut buf)?;
        Ok(buf)
    }

    pub fn read_string(&mut self) -> std::io::Result<String> {
        let mut buf = String::new();
        self.read_to_string(&mut buf)?;
        Ok(buf)
    }
}

impl Read for CDNReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let res = self.resp.read(buf)?;
        self.bandwidth.insert(res as f32);
        Ok(res)
    }
}
