use std::{collections::HashMap, path::PathBuf};

use anyhow::Context;
use handlebars::{handlebars_helper, Handlebars, JsonRender};
use handlebars_misc_helpers::{new_hbs, register};
use napi::{
  assert_type_of,
  bindgen_prelude::FromNapiValue,
  sys::{napi_env, napi_value},
  threadsafe_function::ThreadsafeFunction,
  JsBoolean, JsObject, Result, Status, ValueType,
};
use napi_derive::napi;
use serde::Serialize;
use console::style;

use crate::{
  async_fs,
  constants::{CSSType, CompilerType, FrameworkType, STYLE_EXT_MAP, MEDIA_REGEX},
  utils::normalize_path_str,
};

#[derive(Debug, Clone, Serialize)]
#[napi(object)]
#[serde(rename_all = "camelCase")]
pub struct CreateOptions {
  pub css: CSSType,
  pub css_ext: Option<String>,
  pub framework: FrameworkType,
  pub description: Option<String>,
  pub project_name: String,
  pub version: Option<String>,
  pub date: Option<String>,
  pub typescript: Option<bool>,
  pub template: String,
  pub page_name: String,
  pub compiler: Option<CompilerType>,
  pub set_page_name: Option<String>,
  pub change_ext: Option<bool>,
  pub is_custom_template: Option<bool>,
}

#[derive(Debug)]
pub struct JSReturnObject {
  pub set_page_name: Option<String>,
  pub change_ext: Option<bool>,
}

impl FromNapiValue for JSReturnObject {
  unsafe fn from_napi_value(env: napi_env, napi_val: napi_value) -> Result<Self> {
    let obj = JsObject::from_napi_value(env, napi_val)?;
    let mut js_return_object = JSReturnObject {
      set_page_name: None,
      change_ext: None,
    };
    let has_set_page_name = obj.has_named_property("setPageName")?;
    let has_change_ext = obj.has_named_property("changeExt")?;
    if has_set_page_name {
      js_return_object.set_page_name = Some(obj.get_named_property::<String>("setPageName")?);
    }
    if has_change_ext {
      js_return_object.change_ext = Some(obj.get_named_property::<bool>("changeExt")?);
    }
    Ok(js_return_object)
  }
}

#[derive(Debug)]
pub enum JSReturn {
  Boolean(bool),
  Object(JSReturnObject),
}

impl FromNapiValue for JSReturn {
  unsafe fn from_napi_value(env: napi_env, napi_val: napi_value) -> Result<Self> {
    let is_bool = assert_type_of!(env, napi_val, ValueType::Boolean);
    let is_object = assert_type_of!(env, napi_val, ValueType::Object);
    if let Ok(()) = is_bool {
      let boolean = JsBoolean::from_napi_value(env, napi_val)?;
      return Ok(JSReturn::Boolean(boolean.get_value()?));
    }

    if let Ok(()) = is_object {
      let obj = JSReturnObject::from_napi_value(env, napi_val)?;
      return Ok(JSReturn::Object(obj));
    }

    Err(napi::Error::new(
      Status::InvalidArg,
      "Expected a boolean or object",
    ))
  }
}

handlebars_helper!(includes: |{ s: str = "" }, *args| args.iter().map(|a| a.render()).any(|arg| arg == s));
handlebars_helper!(eq: |x: str, y: str| x == y);

#[derive(Debug, Clone)]
pub struct Creator {
  pub template_root: String,
  pub destination_root: String,
  pub handlebars: Handlebars<'static>,
}

impl Creator {
  pub fn new(template_root: String, destination_root: String) -> Self {
    let handlebars = Self::init_handlebars();
    Creator {
      template_root,
      destination_root,
      handlebars,
    }
  }

  fn init_handlebars() -> Handlebars<'static> {
    let mut handlebars = new_hbs();
    register(&mut handlebars);
    handlebars.register_helper("includes", Box::new(includes));
    handlebars
  }

  pub fn get_template_path(&self, args: &[&str]) -> String {
    let mut file_path = PathBuf::new();
    file_path.push(self.template_root.as_str());
    file_path.push("templates");
    for arg in args {
      file_path.push(arg);
    }
    file_path.to_string_lossy().to_string()
  }

  pub fn get_destination_path(&self, args: &[&str]) -> String {
    let mut file_path = PathBuf::new();
    for arg in args {
      file_path.push(arg);
    }
    if !file_path.is_absolute() {
      file_path = PathBuf::from(self.destination_root.as_str()).join(file_path);
    }
    let ext = file_path.extension();
    if let Some(ext) = ext {
      if ext == "tmpl" {
        file_path.set_extension("");
      }
    }
    let base_name = file_path.file_name().unwrap().to_string_lossy().to_string();
    if base_name.starts_with("_") {
      file_path.set_file_name(&base_name[1..]);
    }
    file_path.to_string_lossy().to_string()
  }

  pub async fn tempate(
    &self,
    from_path: &str,
    dest_path: &str,
    options: &CreateOptions,
  ) -> anyhow::Result<()> {
    if MEDIA_REGEX.is_match(from_path) {
      let dir_name = PathBuf::from(dest_path)
        .parent()
        .unwrap()
        .to_string_lossy()
        .to_string();
      async_fs::create_dir_all(&dir_name)
        .await
        .with_context(|| format!("文件夹创建失败: {}", dir_name))?;
      async_fs::copy(from_path, dest_path)
        .await
        .with_context(|| format!("文件复制失败: {}", from_path))?;
      return Ok(());
    }
    let form_template = async_fs::read(from_path)
      .await
      .with_context(|| format!("文件读取失败: {}", from_path))?;
    let from_template = String::from_utf8_lossy(&form_template);
    let template = if from_template == "" {
      "".to_string()
    } else {
      self
        .handlebars
        .render_template(&from_template, options)
        .with_context(|| format!("模板渲染失败: {}", from_path))?
    };
    let dir_name = PathBuf::from(dest_path)
      .parent()
      .unwrap()
      .to_string_lossy()
      .to_string();
    async_fs::create_dir_all(&dir_name)
      .await
      .with_context(|| format!("文件夹创建失败: {}", dir_name))?;
    async_fs::write(dest_path, template)
      .await
      .with_context(|| format!("文件写入失败: {}", dest_path))?;
    Ok(())
  }

  pub async fn create_files(
    &self,
    files: &[&str],
    template_path: &str,
    options: &mut CreateOptions,
    js_handlers: &HashMap<String, ThreadsafeFunction<CreateOptions>>,
  ) -> anyhow::Result<()> {
    let current_style_ext = STYLE_EXT_MAP.get(&options.css).unwrap_or(&"css");
    options.css_ext = Some(current_style_ext.to_string());
    for file in files {
      let file_relative_path = normalize_path_str(file.replace(template_path, "").as_str());
      let framework = options.framework;
      let is_vue_framework = framework == FrameworkType::Vue || framework == FrameworkType::Vue3;
      if is_vue_framework && file_relative_path.ends_with(".jsx") {
        continue;
      }
      if !is_vue_framework && file_relative_path.ends_with(".vue") {
        continue;
      }
      let mut need_create_file = true;
      let mut page_name = file_relative_path.clone();
      let mut change_ext = true;
      let is_typescript = options.typescript.unwrap_or(false);
      // let is_custom_template = options.is_custom_template.unwrap_or(false);
      if js_handlers.contains_key(&file_relative_path) {
        let js_handler = js_handlers.get(&file_relative_path).unwrap().clone();
        let result = js_handler
          .call_async::<JSReturn>(Ok(options.clone()))
          .await
          .with_context(|| format!("模板自定义函数调用失败: {}", file_relative_path))?;
        match result {
          JSReturn::Boolean(boolean) => {
            need_create_file = boolean;
          }
          JSReturn::Object(obj) => {
            let set_page_name = obj.set_page_name;
            let change_ext_re = obj.change_ext;
            if let Some(set_page_name) = set_page_name {
              page_name = set_page_name;
            }
            if let Some(change_ext_re) = change_ext_re {
              change_ext = change_ext_re;
            }
          }
        };
      }
      if need_create_file {
        let mut dest_re_path = page_name;
        if dest_re_path.starts_with("/") {
          dest_re_path = dest_re_path[1..].to_string();
        }
        if is_typescript
          && change_ext
          && (dest_re_path.ends_with(".js") || dest_re_path.ends_with(".jsx"))
          && !(dest_re_path.ends_with("babel.config.js") || dest_re_path.ends_with(".eslintrc.js"))
        {
          dest_re_path = dest_re_path.replace(".js", ".ts");
        }
        if change_ext && dest_re_path.ends_with(".css") {
          dest_re_path = dest_re_path.replace(".css", format!(".{}", current_style_ext).as_str());
        }
        let file_relative_path = format!("{}{}", template_path, file_relative_path);
        // if is_custom_template {
        //   file_relative_path = format!("{}/{}", template_path, file_relative_path);
        // }
        let dest_path = self.get_destination_path(&[&dest_re_path]);
        let from_path: String = PathBuf::from(file_relative_path)
          .to_string_lossy()
          .to_string();
        self.tempate(from_path.as_str(), dest_path.as_str(), &options.clone()).await?;
        println!("{} {}", style("✔").green(), style("创建文件: ".to_owned() + dest_path.as_str()).color256(238));
      }
    }
    Ok(())
  }
}