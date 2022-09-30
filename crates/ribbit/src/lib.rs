use std::{
    io::{Read, Write},
    net::TcpStream,
};

use mail_parser::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Server {
    EU,
    US,
}

pub enum Command<'a> {
    Summary,
    ProductVersions { product: &'a str },
    ProductCDNs { product: &'a str },
    ProductBGDL { product: &'a str },
    Cert { hash: &'a str },
    Revocation { hash: &'a str },
}

pub fn execute_ribbit_command(server: Server, command: Command) -> Result<Vec<u8>, anyhow::Error> {
    let host = match server {
        Server::EU => "eu.version.battle.net",
        Server::US => "us.version.battle.net",
    };

    let command = match command {
        Command::Summary => String::from("v1/summary"),
        Command::ProductVersions { product } => format!("v1/products/{product}/versions"),
        Command::ProductCDNs { product } => format!("v1/products/{product}/cdns"),
        Command::ProductBGDL { product } => format!("v1/products/{product}/bgdl"),
        Command::Cert { hash } => format!("v1/certs/{hash}"),
        Command::Revocation { hash } => format!("v1/ocsp/{hash}"),
    };

    let mut stream = TcpStream::connect((host, 1119))?;
    write!(stream, "{}\r\n", command)?;

    let mut reply = vec![];
    stream.read_to_end(&mut reply)?;

    Ok(reply)
}

fn get_body_with_content_disposition(res: &[u8], content_disposition: &str) -> Option<String> {
    let parsed = Message::parse(res).unwrap();
    let summary = parsed.parts.iter().find(|part| {
        part.headers()
            .iter()
            .find(|h| h.name() == "Content-Disposition")
            .map(|v| {
                v.value()
                    .as_content_type_ref()
                    .map(|c| c.get_type() == content_disposition)
                    .is_some()
            })
            .unwrap_or(false)
    })?;
    summary.get_text_contents().map(|s| s.to_string())
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub product: String,
    pub seqn: u32,
    pub flags: String,
}

pub fn summary(server: Server) -> Result<Vec<Endpoint>, anyhow::Error> {
    let res = execute_ribbit_command(server, Command::Summary)?;
    let body = get_body_with_content_disposition(&res, "summary")
        .expect("no mime section with content-disposition = summary");

    let mut lines = body.lines();
    let _header = lines.next().expect("header not present");

    let mut res = vec![];
    for line in lines {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        let mut parts = line.split('|');
        res.push(Endpoint {
            product: parts.next().expect("no product present").to_string(),
            seqn: parts.next().expect("no seqn present").parse()?,
            flags: parts.next().expect("no flags present").to_string(),
        });
    }

    Ok(res)
}

#[derive(Debug, Clone)]
pub struct Version {
    pub region: String,
    pub build_config: String, // HEX 16
    pub cdn_config: String,   // HEX 16
    pub key_ring: String,     // HEX 16
    pub build_id: u32,
    pub versions_name: String,
    pub product_config: String, // HEX 16
}

pub fn versions(server: Server, product: &str) -> Result<Vec<Version>, anyhow::Error> {
    let res = execute_ribbit_command(server, Command::ProductVersions { product })?;
    let body = get_body_with_content_disposition(&res, "version")
        .expect("no mime section with content-disposition = version");

    let mut lines = body.lines();
    let _header = lines.next().expect("header not present");

    let mut res = vec![];
    for line in lines {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        let mut parts = line.split('|');
        res.push(Version {
            region: parts.next().expect("no region present").to_string(),
            build_config: parts.next().expect("no build_config present").to_string(),
            cdn_config: parts.next().expect("no cdn_config present").to_string(),
            key_ring: parts.next().expect("no key_ring present").to_string(),
            build_id: parts.next().expect("no build_id present").parse()?,
            versions_name: parts.next().expect("no versions_name present").to_string(),
            product_config: parts.next().expect("no product_config present").to_string(),
        });
    }

    Ok(res)
}

#[derive(Debug, Clone)]
pub struct CDNS {
    pub name: String,
    pub path: String,
    pub hosts: Vec<String>,
    pub servers: Vec<String>,
    pub config_path: String,
}

pub fn cdns(server: Server, product: &str) -> Result<Vec<CDNS>, anyhow::Error> {
    let res = execute_ribbit_command(server, Command::ProductCDNs { product })?;
    let body = get_body_with_content_disposition(&res, "cdn")
        .expect("no mime section with content-disposition = cdn");

    let mut lines = body.lines();
    let _header = lines.next().expect("header not present");

    let mut res = vec![];
    for line in lines {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        let mut parts = line.split('|');
        res.push(CDNS {
            name: parts.next().expect("no name present").to_string(),
            path: parts.next().expect("no path present").to_string(),
            hosts: parts
                .next()
                .expect("no hosts present")
                .split_ascii_whitespace()
                .map(str::to_string)
                .collect(),
            servers: parts
                .next()
                .expect("no servers present")
                .split_ascii_whitespace()
                .map(str::to_string)
                .collect(),
            config_path: parts.next().expect("no config path present").to_string(),
        });
    }

    Ok(res)
}

pub fn bgdl(server: Server, product: &str) -> Result<Vec<Version>, anyhow::Error> {
    let res = execute_ribbit_command(server, Command::ProductBGDL { product })?;
    let body = get_body_with_content_disposition(&res, "version")
        .expect("no mime section with content-disposition = version");

    let mut lines = body.lines();
    let _header = lines.next().expect("header not present");

    let mut res = vec![];
    for line in lines {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        let mut parts = line.split('|');
        res.push(Version {
            region: parts.next().expect("no region present").to_string(),
            build_config: parts.next().expect("no build_config present").to_string(),
            cdn_config: parts.next().expect("no cdn_config present").to_string(),
            key_ring: parts.next().expect("no key_ring present").to_string(),
            build_id: parts.next().expect("no build_id present").parse()?,
            versions_name: parts.next().expect("no versions_name present").to_string(),
            product_config: parts.next().expect("no product_config present").to_string(),
        });
    }

    Ok(res)
}
