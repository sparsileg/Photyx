// commands/session.rs — Session state and crash recovery Tauri command handlers

use std::sync::Arc;
use tauri::State;
use crate::PhotoxState;

#[tauri::command]
pub fn get_session(state: State<Arc<PhotoxState>>) -> serde_json::Value {
    let ctx = state.context.lock().expect("context lock poisoned");
    serde_json::json!({
        "fileList":     ctx.file_list,
        "currentFrame": ctx.current_frame,
    })
}

#[tauri::command]
pub fn get_variable(name: String, state: State<Arc<PhotoxState>>) -> Option<String> {
    let ctx = state.context.lock().expect("context lock poisoned");
    ctx.variables.get(&name.to_uppercase())
        .or_else(|| ctx.variables.get(&name))
        .cloned()
}

#[tauri::command]
pub fn debug_buffer_info(state: State<Arc<PhotoxState>>) -> serde_json::Value {
    let ctx = state.context.lock().expect("context lock poisoned");
    let path = ctx.file_list.get(ctx.current_frame).cloned();
    let buffer_info = path.as_ref().and_then(|p| ctx.image_buffers.get(p)).map(|b| {
        serde_json::json!({
            "filename":      b.filename,
            "width":         b.width,
            "height":        b.height,
            "display_width": b.display_width,
            "bit_depth":     format!("{:?}", b.bit_depth),
            "color_space":   format!("{:?}", b.color_space),
            "channels":      b.channels,
            "has_pixels":    b.pixels.is_some(),
            "pixel_type":    b.pixels.as_ref().map(|p| match p {
                crate::context::PixelData::U8(_)  => "U8",
                crate::context::PixelData::U16(_) => "U16",
                crate::context::PixelData::F32(_) => "F32",
            }),
        })
    });
    serde_json::json!({
        "current_frame": ctx.current_frame,
        "file_count":    ctx.file_list.len(),
        "buffer":        buffer_info,
    })
}


#[tauri::command]
pub fn get_keywords(state: State<Arc<PhotoxState>>) -> serde_json::Value {
    let ctx = state.context.lock().expect("context lock poisoned");
    let path = match ctx.file_list.get(ctx.current_frame) {
        Some(p) => p,
        None => return serde_json::json!({}),
    };
    let buffer = match ctx.image_buffers.get(path) {
        Some(b) => b,
        None => return serde_json::json!({}),
    };
    let mut map = serde_json::Map::new();
    for kw in buffer.keywords.values() {
        map.insert(kw.name.clone(), serde_json::json!({
            "name":    kw.name,
            "value":   kw.value,
            "comment": kw.comment,
        }));
    }
    serde_json::Value::Object(map)
}

// ----------------------------------------------------------------------
