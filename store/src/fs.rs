//!
//! File system abstraction layer. Currently supporting storage on the filesystem
//! and the browser domain-associated local storage ([Web Storage API](https://developer.mozilla.org/en-US/docs/Web/API/Web_Storage_API)).
//!
//! Storage APIs abstracted:
//! - Std File I/O (fs::xxx)
//! - NodeJS File I/O (fs::read_file_sync)
//! - Local Storage
//!
//! By default, all I/O functions will use the name of the file as a key
//! for localstorage. If you want to manually specify the localstorage key,
//! you should use `*_with_localstorage()` suffixed functions.
//!

use crate::result::Result;
use cfg_if::cfg_if;
use js_sys::Function;
#[allow(unused_imports)]
use js_sys::{Object, Reflect};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};
use wasm_bindgen::prelude::*;
use workflow_core::runtime;
use workflow_wasm::object::ObjectTrait;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen]
    pub fn require(s: &str) -> JsValue;
}

pub struct Fs {
    pub ctx: JsValue,
    pub exists_sync: Function,
    pub write_file_sync: Function,
    pub read_file_sync: Function,
    pub mkdir_sync: Function,
    pub unlink_sync: Function,
}

static mut FS: Option<Fs> = None;
#[inline]
fn fs() -> &'static Fs {
    unsafe {
        if FS.is_none() {
            let ctx = require("fs");
            let fs = Object::from(ctx.clone());
            let static_fs = Fs {
                ctx,
                exists_sync: Function::from(fs.get("existsSync").unwrap()),
                write_file_sync: Function::from(fs.get("writeFileSync").unwrap()),
                read_file_sync: Function::from(fs.get("readFileSync").unwrap()),
                mkdir_sync: Function::from(fs.get("mkdirSync").unwrap()),
                unlink_sync: Function::from(fs.get("unlinkSync").unwrap()),
            };
            FS.replace(static_fs);
        }

        FS.as_ref().unwrap()
    }
}

#[inline]
pub fn exists_sync(file: &str) -> Result<bool> {
    let Fs {
        ctx, exists_sync, ..
    } = fs();
    let ret = exists_sync.call1(ctx, &JsValue::from_str(file)).unwrap();
    Ok(ret.as_bool().unwrap())
}

#[inline]
pub fn write_file_sync(file: &str, data: &str, options: Object) -> Result<()> {
    let Fs {
        ctx,
        write_file_sync,
        ..
    } = fs();
    write_file_sync.call3(
        ctx,
        &JsValue::from_str(file),
        &JsValue::from_str(data),
        &options,
    )?;
    Ok(())
}

#[inline]
pub fn read_file_sync(file: &str, options: Object) -> Result<JsValue> {
    let Fs {
        ctx,
        read_file_sync,
        ..
    } = fs();
    let ret = read_file_sync.call2(ctx, &JsValue::from_str(file), &options)?;
    Ok(ret)
}

#[inline]
pub fn mkdir_sync(dir: &str, options: Object) -> Result<()> {
    let Fs {
        ctx, mkdir_sync, ..
    } = fs();
    mkdir_sync.call2(ctx, &JsValue::from_str(dir), &options)?;
    Ok(())
}

#[inline]
pub fn unlink_sync(file: &str) -> Result<()> {
    let Fs {
        ctx, unlink_sync, ..
    } = fs();
    unlink_sync.call1(ctx, &JsValue::from_str(file))?;
    Ok(())
}

//
// using `module="fs"` results in an `import` statement
// generated by wasm-bindgen+wasm-pack;  eventually it
// will be possible to use `cfg()` to determine the build
// target (web vs. nodejs). Until this is merged and stabilized,
// we are using manual bindings (above) that will later become
// proxy functions for native bindings.
//
// #[wasm_bindgen(module = "fs")]
// extern "C" {
//     #[wasm_bindgen(js_name = existsSync)]
//     pub fn exists_sync(file: &str) -> bool;
//     #[wasm_bindgen(js_name = writeFileSync)]
//     pub fn write_file_sync(file: &str, data: &str, options: Object);
//     #[wasm_bindgen(js_name = readFileSync)]
//     pub fn read_file_sync(file: &str, options: Object) -> JsValue;
//     #[wasm_bindgen(js_name = mkdirSync)]
//     pub fn mkdir_sync(dir: &str, options: Object);
//     #[wasm_bindgen(js_name = unlinkSync)]
//     pub fn unlink_sync(file: &str);
// }
//

pub fn local_storage() -> web_sys::Storage {
    web_sys::window().unwrap().local_storage().unwrap().unwrap()
}

#[derive(Default)]
pub struct Options {
    pub local_storage_key: Option<String>,
}

impl Options {
    pub fn with_local_storage_key(key: &str) -> Self {
        Options {
            local_storage_key: Some(key.to_string()),
        }
    }

    pub fn local_storage_key(&self, filename: &Path) -> String {
        self.local_storage_key
            .clone()
            .unwrap_or(filename.file_name().unwrap().to_str().unwrap().to_string())
    }
}

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {

        use crate::error::Error;

        pub async fn exists_with_options(filename: &Path, options : Options) -> Result<bool> {
            if runtime::is_node() || runtime::is_nw() {
                let filename = filename.to_string_lossy().to_string();
                Ok(exists_sync(&filename)?)
            } else {
                let key_name = options.local_storage_key(filename);
                Ok(local_storage().get_item(&key_name)?.is_some())
            }
        }

        pub async fn read_to_string_with_options(filename: &Path, options : Options) -> Result<String> {
            if runtime::is_node() || runtime::is_nw() {
                let filename = filename.to_string_lossy().to_string();
                let options = Object::new();
                Reflect::set(&options, &"encoding".into(), &"utf-8".into())?;
                // options.set("encoding", "utf-8");
                let js_value = read_file_sync(&filename, options)?;
                let text = js_value.as_string().ok_or(Error::DataIsNotAString(filename))?;
                Ok(text)
            } else {
                let key_name = options.local_storage_key(filename);
                if let Some(text) = local_storage().get_item(&key_name)? {
                    Ok(text)
                } else {
                    Err(Error::NotFound(filename.to_string_lossy().to_string()))
                }
            }
        }

        pub async fn write_string_with_options(filename: &Path, options: Options, text : &str) -> Result<()> {
            if runtime::is_node() || runtime::is_nw() {
                let filename = filename.to_string_lossy().to_string();
                let options = Object::new();
                write_file_sync(&filename, text, options)?;
            } else {
                let key_name = options.local_storage_key(filename);
                local_storage().set_item(&key_name, text)?;
            }
            Ok(())
        }

        pub async fn remove_with_options(filename: &Path, options: Options) -> Result<()> {
            if runtime::is_node() || runtime::is_nw() {
                let filename = filename.to_string_lossy().to_string();
                // let options = Object::new();
                unlink_sync(&filename)?;
            } else {
                let key_name = options.local_storage_key(filename);
                local_storage().remove_item(&key_name)?;
            }
            Ok(())
        }

        pub async fn create_dir_all(filename: &Path) -> Result<()> {
            if runtime::is_node() || runtime::is_nw() {
                let options = Object::new();
                Reflect::set(&options, &JsValue::from("recursive"), &JsValue::from_bool(true))?;
                let filename = filename.to_string_lossy().to_string();
                mkdir_sync(&filename, options)?;
            }

            Ok(())
        }

    } else {

        // native platforms

        pub async fn exists_with_options(filename: &Path, _options: Options) -> Result<bool> {
            Ok(filename.exists())
        }

        pub async fn read_to_string_with_options(filename: &Path, _options: Options) -> Result<String> {
            Ok(std::fs::read_to_string(filename)?)
        }

        pub async fn write_string_with_options(filename: &Path, _options: Options, text : &str) -> Result<()> {
            Ok(std::fs::write(filename, text)?)
        }

        pub async fn remove_with_options(filename: &Path, _options: Options) -> Result<()> {
            std::fs::remove_file(filename)?;
            Ok(())
        }

        pub async fn create_dir_all(dir: &Path) -> Result<()> {
            std::fs::create_dir_all(dir)?;
            Ok(())
        }

    }

}

pub async fn exists(filename: &Path) -> Result<bool> {
    exists_with_options(filename, Options::default()).await
}

pub async fn read_to_string(filename: &Path) -> Result<String> {
    read_to_string_with_options(filename, Options::default()).await
}

pub async fn write_string(filename: &Path, text: &str) -> Result<()> {
    write_string_with_options(filename, Options::default(), text).await
}

pub async fn remove(filename: &Path) -> Result<()> {
    remove_with_options(filename, Options::default()).await
}

pub async fn read_json_with_options<T>(filename: &Path, options: Options) -> Result<T>
where
    T: DeserializeOwned,
{
    let text = read_to_string_with_options(filename, options).await?;
    Ok(serde_json::from_str(&text)?)
}

pub async fn write_json_with_options<T>(filename: &Path, options: Options, value: &T) -> Result<()>
where
    T: Serialize,
{
    let json = serde_json::to_string(value)?;
    write_string_with_options(filename, options, &json).await?;
    Ok(())
}

pub async fn read_json<T>(filename: &Path) -> Result<T>
where
    T: DeserializeOwned,
{
    read_json_with_options(filename, Options::default()).await
}

pub async fn write_json<T>(filename: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    write_json_with_options(filename, Options::default(), value).await
}

pub fn resolve_path(path: &str) -> PathBuf {
    if let Some(_stripped) = path.strip_prefix("~/") {
        if runtime::is_web() {
            PathBuf::from(path)
        } else if runtime::is_node() || runtime::is_nw() {
            todo!();
        } else {
            cfg_if! {
                if #[cfg(target_arch = "wasm32")] {
                    PathBuf::from(path)
                } else {
                    home::home_dir().unwrap().join(_stripped)
                }
            }
        }
    } else {
        PathBuf::from(path)
    }
}
