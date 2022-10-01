use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// TODO: Verify if there's sane defaults for Option<bool> fields

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub categories: Categories,
    pub connection_strings: Vec<ConnectionString>,
    pub environments: Vec<Environment>,
    pub files: PerLocale<Files>,
    pub fragment_id: String,
    pub fragments: Vec<Fragment>,
    pub locales: Vec<String>,
    pub strings: PerLocale<Strings>,
    pub types: Types,
    #[serde(default = "HashMap::new")]
    pub vars: Vars,
    pub version: i64,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogFragment {
    // TODO: Clear up overlap
    pub files: Option<PerLocale<Files>>,
    pub fragment_id: String,
    pub strings: Option<PerLocale<Strings>>,
    pub types: Option<Types>,
    pub version: i64,
    #[serde(default = "HashMap::new")]
    pub installs: Installs,
    #[serde(default = "Vec::new")]
    pub presence_resources: Vec<PresenceResource>,
    #[serde(default = "Vec::new")]
    pub products: Vec<Product>,
    // TODO: Another nested expression-like type
    #[serde(default = "HashMap::new")]
    pub program_configuration: HashMap<String, Value>,
    #[serde(default = "Vec::new")]
    pub features: Vec<Feature>,
    #[serde(default = "HashMap::new")]
    pub vars: Vars,
    pub categories: Option<Categories>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Categories {
    pub definitions: Vec<Definition>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub id: String,
    pub name: String,
    pub rank: i64,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionString {
    pub global: Vec<Global>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Global {
    pub aurora: String,
    pub id: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub aurora: String,
    pub id: String,
    pub login_server: String,
    pub name: String,
    pub short_name: String,
    pub tag: String,
    pub hidden: Option<bool>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Types {
    pub definitions: Vec<Definition2>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition2 {
    pub category: String,
    pub id: String,
    pub product_defaults: Option<ProductDefaults>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductDefaults {
    pub name_style: Option<String>,
    pub rank: Option<i64>,
    pub supports_starter_mode: Option<bool>,
    pub type_name: Option<String>,
    pub ribbon_text: Option<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fragment {
    pub hash: String,
    pub name: String,
    // TODO: Nested requirement expression format
    pub requires: Option<Value>,
    pub decryption_key_id: Option<String>,
    pub encrypted_hash: Option<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub hash: String,
    pub name: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerLocale<T> {
    pub default: Option<T>,
    #[serde(rename = "deDE")]
    pub de_de: Option<T>,
    #[serde(rename = "esES")]
    pub es_es: Option<T>,
    #[serde(rename = "esMX")]
    pub es_mx: Option<T>,
    #[serde(rename = "frFR")]
    pub fr_fr: Option<T>,
    #[serde(rename = "itIT")]
    pub it_it: Option<T>,
    #[serde(rename = "jaJP")]
    pub ja_jp: Option<T>,
    #[serde(rename = "koKR")]
    pub ko_kr: Option<T>,
    #[serde(rename = "plPL")]
    pub pl_pl: Option<T>,
    #[serde(rename = "ptBR")]
    pub pt_br: Option<T>,
    #[serde(rename = "ptPT")]
    pub pt_pt: Option<T>,
    #[serde(rename = "ruRU")]
    pub ru_ru: Option<T>,
    #[serde(rename = "thTH")]
    pub th_th: Option<T>,
    #[serde(rename = "zhCN")]
    pub zh_cn: Option<T>,
    #[serde(rename = "zhTW")]
    pub zh_tw: Option<T>,
}

pub type Vars = HashMap<String, String>;
pub type Files = HashMap<String, Resource>;
pub type Strings = HashMap<String, String>;
pub type Installs = HashMap<String, InstallItem>;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallItem {
    pub tact_product: String,
    pub requires_sso_token: Option<bool>,
    pub auto_update_policy: Option<AutoUpdatePolicy>,
    pub sso_launch_argument: Option<String>,
    pub serialize_install_data_in_launch_options: Option<bool>,
    pub locale_specific_uid: Option<bool>,
    pub supports_streaming: Option<bool>,
    pub deprecated: Option<bool>,
    pub run_64_bit_default: Option<bool>,
    pub run_64_bit_only: Option<bool>,
    pub uses_web_credentials: Option<bool>,
    #[serde(default = "Vec::new")]
    pub additional_tags: Vec<InstallTag>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoUpdatePolicy {
    pub meets_criteria: MeetsCriteria,
    #[serde(default = "Vec::new")]
    pub requires_licenses: Vec<i64>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeetsCriteria {
    pub actioned_in_last_n_seconds: Option<i64>,
    pub played_in_last_n_seconds: i64,
    pub has_game_time: Option<bool>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallTag {
    pub license_id: Option<i64>,
    pub igr: Option<bool>,
    pub tags: Vec<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresenceResource {
    pub display_name: String,
    pub icon_16: String,
    pub icon_275: String,
    pub icon_32: String,
    pub icon_56: String,
    pub icon_svg: String,
    pub program_id: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub base: Base,
    pub id: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Base {
    pub breaking_news_url: Option<String>,
    pub content_id: Option<String>,
    pub default_product_type: Option<String>,
    pub game_background: Option<String>,
    pub game_icon_svg: Option<String>,
    pub genre: Option<String>,
    pub icon_medium: Option<String>,
    pub icon_small: Option<String>,
    pub icon_tiny: Option<String>,
    pub install_background_v2: Option<String>,
    pub key_art: Option<String>,
    pub login_background: Option<String>,
    pub login_color: Option<String>,
    pub logo_v2: Option<String>,
    #[serde(default = "Vec::new")]
    pub misc_flags: Vec<String>,
    pub mobile_promo_text: Option<String>,
    pub mobile_qr_code: Option<String>,
    pub mobile_qr_code_text: Option<String>,
    #[serde(default = "Vec::new")]
    pub mobile_stores: Vec<MobileStore>,
    pub name: Option<String>,
    pub program_id: Option<String>,
    #[serde(default = "Vec::new")]
    pub quick_links: Vec<QuickLink>,
    pub region_permission_flags: Option<RegionPermissionFlags>,
    #[serde(default = "Vec::new")]
    pub starter_items: Vec<StarterItem>,
    pub starter_mode_when_offline: Option<bool>,
    #[serde(default = "Vec::new")]
    pub supported_platforms: Vec<String>,
    #[serde(default = "Vec::new")]
    pub supported_regions: Vec<String>,
    pub supports_language_selection: Option<bool>,
    pub supports_starter_mode: Option<bool>,
    pub tab_order: Option<i64>,
    pub title_id: Option<i64>,
    pub types: Option<ProductTypes>,
    pub unsupported_platform_behavior: Option<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MobileStore {
    pub description: String,
    pub logo: String,
    pub url: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct QuickLink {
    pub id: String,
    pub hidden: Option<bool>,
    pub action: Option<Action>,
    pub icon_v2: Option<String>,
    pub label: Option<String>,
    pub rank: Option<i64>,
    pub show_on_web_ui: Option<bool>,
    pub icon: Option<String>,
    pub color: Option<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    #[serde(rename = "type")]
    pub type_field: String,
    pub target: Option<String>,
    pub url: Option<String>,
    pub custom_action_id: Option<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RegionPermissionFlags {
    pub default: Option<String>,
    #[serde(default = "Vec::new")]
    pub overrides: Vec<Override>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Override {
    #[serde(rename = "CN")]
    pub cn: Option<String>,
    pub override_type: String,
    #[serde(rename = "BLR")]
    pub blr: Option<String>,
    #[serde(rename = "RUS")]
    pub rus: Option<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct StarterItem {
    pub id: String,
    #[serde(rename = "type")]
    pub type_field: String,
    pub hidden: Option<bool>,
    pub action: Option<Action>,
    pub label: Option<String>,
    pub rank: Option<i64>,
    pub style: Option<String>,
}

pub type ProductTypes = HashMap<String, ProductType>;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ProductType {
    pub uid: Option<String>,
    pub auto_favorite_on_grant: Option<bool>,
    #[serde(default = "Vec::new")]
    pub supported_regions: Vec<String>,
    pub supports_starter_mode: Option<bool>,
    pub install_disabled_message: Option<String>,
    pub play_disabled_message: Option<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub id: String,
    pub requires: Value,
}
