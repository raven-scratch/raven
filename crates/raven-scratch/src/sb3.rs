//! The `project.json` data model.
//!
//! Field names, key order and optionality follow what
//! `packages/scratch-vm/src/serialization/sb3.js` writes, so a raven-asm build is
//! byte-for-byte the kind of file the Scratch editor itself saves.

use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The version string raven-asm writes into `meta.semver`.
pub const SB3_SEMVER: &str = "3.0.0";
/// The VM version Scratch 3.0 projects traditionally record.
pub const SB3_VM_VERSION: &str = "0.2.0";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Project {
    pub targets: Vec<Target>,
    pub monitors: Vec<Monitor>,
    pub extensions: Vec<String>,
    #[serde(rename = "extensionURLs", skip_serializing_if = "BTreeMap::is_empty")]
    pub extension_urls: BTreeMap<String, String>,
    pub meta: Meta,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Meta {
    pub semver: String,
    pub vm: String,
    pub agent: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Target {
    pub is_stage: bool,
    pub name: String,
    pub variables: BTreeMap<String, Vec<Value>>,
    pub lists: BTreeMap<String, Vec<Value>>,
    pub broadcasts: BTreeMap<String, String>,
    pub blocks: BTreeMap<String, Value>,
    pub comments: BTreeMap<String, Value>,
    pub current_costume: u32,
    pub costumes: Vec<Costume>,
    pub sounds: Vec<Sound>,
    pub volume: f64,
    pub layer_order: i64,

    // Stage only.
    pub tempo: Option<u32>,
    pub video_transparency: Option<u32>,
    pub video_state: Option<String>,
    /// Written as `null` for the stage, absent for sprites.
    pub text_to_speech_language: Option<Option<String>>,

    // Sprite only.
    pub visible: Option<bool>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub size: Option<f64>,
    pub direction: Option<f64>,
    pub draggable: Option<bool>,
    pub rotation_style: Option<String>,
}

impl Target {
    pub fn new_stage(name: impl Into<String>) -> Self {
        Target {
            is_stage: true,
            name: name.into(),
            volume: 100.0,
            layer_order: 0,
            current_costume: 0,
            tempo: Some(60),
            video_transparency: Some(50),
            video_state: Some("off".to_string()),
            text_to_speech_language: Some(None),
            ..Target::default()
        }
    }

    pub fn new_sprite(name: impl Into<String>, layer_order: i64) -> Self {
        Target {
            is_stage: false,
            name: name.into(),
            volume: 100.0,
            layer_order,
            current_costume: 0,
            visible: Some(true),
            x: Some(0.0),
            y: Some(0.0),
            size: Some(100.0),
            direction: Some(90.0),
            draggable: Some(false),
            rotation_style: Some("all around".to_string()),
            ..Target::default()
        }
    }
}

impl Serialize for Target {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("isStage", &self.is_stage)?;
        m.serialize_entry("name", &self.name)?;
        m.serialize_entry("variables", &self.variables)?;
        m.serialize_entry("lists", &self.lists)?;
        m.serialize_entry("broadcasts", &self.broadcasts)?;
        m.serialize_entry("blocks", &self.blocks)?;
        m.serialize_entry("comments", &self.comments)?;
        m.serialize_entry("currentCostume", &self.current_costume)?;
        m.serialize_entry("costumes", &self.costumes)?;
        m.serialize_entry("sounds", &self.sounds)?;
        m.serialize_entry("volume", &self.volume)?;
        m.serialize_entry("layerOrder", &self.layer_order)?;

        if self.is_stage {
            m.serialize_entry("tempo", &self.tempo.unwrap_or(60))?;
            m.serialize_entry("videoTransparency", &self.video_transparency.unwrap_or(50))?;
            m.serialize_entry("videoState", self.video_state.as_deref().unwrap_or("off"))?;
            m.serialize_entry(
                "textToSpeechLanguage",
                &self.text_to_speech_language.clone().unwrap_or(None),
            )?;
        } else {
            m.serialize_entry("visible", &self.visible.unwrap_or(true))?;
            m.serialize_entry("x", &self.x.unwrap_or(0.0))?;
            m.serialize_entry("y", &self.y.unwrap_or(0.0))?;
            m.serialize_entry("size", &self.size.unwrap_or(100.0))?;
            m.serialize_entry("direction", &self.direction.unwrap_or(90.0))?;
            m.serialize_entry("draggable", &self.draggable.unwrap_or(false))?;
            m.serialize_entry(
                "rotationStyle",
                self.rotation_style.as_deref().unwrap_or("all around"),
            )?;
        }
        m.end()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Costume {
    #[serde(rename = "assetId")]
    pub asset_id: String,
    pub name: String,
    #[serde(rename = "bitmapResolution", skip_serializing_if = "Option::is_none")]
    pub bitmap_resolution: Option<u32>,
    pub md5ext: String,
    #[serde(rename = "dataFormat")]
    pub data_format: String,
    #[serde(rename = "rotationCenterX")]
    pub rotation_center_x: f64,
    #[serde(rename = "rotationCenterY")]
    pub rotation_center_y: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Sound {
    #[serde(rename = "assetId")]
    pub asset_id: String,
    pub name: String,
    #[serde(rename = "dataFormat")]
    pub data_format: String,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<u32>,
    #[serde(rename = "sampleCount", skip_serializing_if = "Option::is_none")]
    pub sample_count: Option<u32>,
    pub md5ext: String,
}

/// A variable/list monitor record. Scratch needs one of these for
/// `data_showvariable` / `data_showlist` to be able to reveal the monitor.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Monitor {
    pub id: String,
    pub mode: String,
    pub opcode: String,
    pub params: BTreeMap<String, String>,
    #[serde(rename = "spriteName")]
    pub sprite_name: Option<String>,
    pub value: Value,
    pub width: f64,
    pub height: f64,
    /// `null` asks the editor to auto-position the monitor, which is what the
    /// VM's `MonitorRecord` default means.
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub visible: bool,
    #[serde(rename = "sliderMin", skip_serializing_if = "Option::is_none")]
    pub slider_min: Option<f64>,
    #[serde(rename = "sliderMax", skip_serializing_if = "Option::is_none")]
    pub slider_max: Option<f64>,
    #[serde(rename = "isDiscrete", skip_serializing_if = "Option::is_none")]
    pub is_discrete: Option<bool>,
}
