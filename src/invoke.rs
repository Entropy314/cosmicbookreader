use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::types::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

fn js_err(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| "Unknown JS error".to_string())
}

async fn call<A: Serialize, R: for<'de> serde::Deserialize<'de>>(
    cmd: &str,
    args: &A,
) -> Result<R, String> {
    let args_js = serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())?;
    let result = invoke(cmd, args_js).await.map_err(js_err)?;
    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}

pub async fn pick_directory() -> Result<Option<String>, String> {
    #[derive(Serialize)]
    struct Args {}
    call("pick_directory", &Args {}).await
}

pub async fn scan_directory(path: &str) -> Result<Vec<ComicBook>, String> {
    #[derive(Serialize)]
    struct Args<'a> {
        path: &'a str,
    }
    call("scan_directory", &Args { path }).await
}

pub async fn refresh_library() -> Result<Vec<ComicBook>, String> {
    #[derive(Serialize)]
    struct Args {}
    call("refresh_library", &Args {}).await
}

pub async fn get_library() -> Result<Vec<ComicBook>, String> {
    #[derive(Serialize)]
    struct Args {}
    call("get_library", &Args {}).await
}

pub async fn get_cover(comic_id: &str) -> Result<Option<String>, String> {
    // Tauri 2 maps snake_case args to camelCase in IPC
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args<'a> {
        comic_id: &'a str,
    }
    call("get_cover", &Args { comic_id }).await
}

pub async fn open_comic(comic_id: &str) -> Result<OpenComicResult, String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args<'a> {
        comic_id: &'a str,
    }
    call("open_comic", &Args { comic_id }).await
}

pub async fn get_page(comic_id: &str, page_index: u32) -> Result<PageData, String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args<'a> {
        comic_id: &'a str,
        page_index: u32,
    }
    call("get_page", &Args { comic_id, page_index }).await
}

pub async fn close_comic(comic_id: &str) -> Result<(), String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args<'a> {
        comic_id: &'a str,
    }
    call("close_comic", &Args { comic_id }).await
}

pub async fn toggle_fullscreen() -> Result<bool, String> {
    #[derive(Serialize)]
    struct Args {}
    call("toggle_fullscreen", &Args {}).await
}

pub async fn save_progress(comic_id: &str, page: u32) -> Result<(), String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args<'a> {
        comic_id: &'a str,
        page: u32,
    }
    call("save_progress", &Args { comic_id, page }).await
}

pub async fn pick_files() -> Result<Vec<ComicBook>, String> {
    #[derive(Serialize)]
    struct Args {}
    call("pick_files", &Args {}).await
}

pub async fn remove_comic(comic_id: &str) -> Result<(), String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args<'a> {
        comic_id: &'a str,
    }
    call("remove_comic", &Args { comic_id }).await
}

pub async fn delete_comic_file(comic_id: &str) -> Result<(), String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args<'a> {
        comic_id: &'a str,
    }
    call("delete_comic_file", &Args { comic_id }).await
}
