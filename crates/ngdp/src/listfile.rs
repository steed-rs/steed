use std::collections::HashMap;

pub struct ListFile {
    map: HashMap<String, i32>,
}

impl ListFile {
    pub fn get_id(&self, path: &str) -> Option<i32> {
        self.map.get(&path.to_lowercase()).cloned()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &i32)> {
        self.map.iter()
    }
}

pub fn parse_listfile(content: &str) -> Result<ListFile, anyhow::Error> {
    let mut map = HashMap::new();
    for line in content.lines() {
        if line.is_empty() {
            continue;
        }

        let (id, path) = match line.split_once(';') {
            Some(v) => v,
            None => {
                eprintln!("Bad line in listfile, skipping... {}", line);
                continue;
            }
        };

        let id: i32 = match id.parse() {
            Ok(v) => v,
            Err(_) => {
                eprintln!("Bad line in listfile, skipping... {}", line);
                continue;
            }
        };

        map.insert(path.to_lowercase(), id);
    }

    Ok(ListFile { map })
}
