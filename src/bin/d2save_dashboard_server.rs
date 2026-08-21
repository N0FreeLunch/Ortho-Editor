use d2r_core::engine::formatter::format_item;
use d2r_core::item::{HuffmanTree, Item, ItemProperty};
use d2r_core::save::Save;
use d2r_core::verify::alpha_inventory_routing::{alpha_inventory_route, AlphaInventoryRoute};
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

const MAX_SAVE_BYTES: usize = 16 * 1024 * 1024;

fn main() -> std::io::Result<()> {
    let port = std::env::args()
        .skip(1)
        .find_map(|arg| arg.strip_prefix("--port=").and_then(|v| v.parse().ok()))
        .unwrap_or(8765);
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    eprintln!("dashboard server listening on http://127.0.0.1:{port}");
    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let _ = handle(stream);
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    let request = read_request(&mut stream)?;
    let Some(split) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
        return reply(&mut stream, 400, "text/plain", b"Malformed request");
    };
    let (head, body) = request.split_at(split + 4);
    let head = String::from_utf8_lossy(head);
    let mut words = head.lines().next().unwrap_or_default().split_whitespace();
    let method = words.next().unwrap_or_default();
    let path = words.next().unwrap_or_default();
    match (method, path) {
        ("POST", "/api/parse-save") => {
            if body.len() > MAX_SAVE_BYTES {
                return reply(
                    &mut stream,
                    413,
                    "application/json",
                    br#"{"error":"save too large"}"#,
                );
            }
            match dashboard_payload(body) {
                Ok(payload) => reply(
                    &mut stream,
                    200,
                    "application/json; charset=utf-8",
                    serde_json::to_string(&payload).unwrap().as_bytes(),
                ),
                Err(error) => reply(
                    &mut stream,
                    422,
                    "application/json; charset=utf-8",
                    json!({"error": error}).to_string().as_bytes(),
                ),
            }
        }
        ("GET", "/") | ("GET", "/index.html") => serve_file(
            &mut stream,
            frontend_root().join("index.html"),
            "text/html; charset=utf-8",
        ),
        ("GET", path) if path.starts_with("/fixtures/") && !path.contains("..") => serve_file(
            &mut stream,
            fixture_root().join(&path[10..]),
            "application/octet-stream",
        ),
        _ => reply(&mut stream, 404, "text/plain", b"Not found"),
    }
}

fn serve_file(stream: &mut TcpStream, path: PathBuf, content_type: &str) -> std::io::Result<()> {
    match fs::read(path) {
        Ok(bytes) => reply(stream, 200, content_type, &bytes),
        Err(_) => reply(stream, 404, "text/plain", b"Not found"),
    }
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/savegames/original")
}

fn frontend_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/dashboard")
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(request);
        }
        request.extend_from_slice(&buffer[..read]);
        let Some(split) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..split + 4]);
        let length = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("Content-Length:")
                    .or_else(|| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        if request.len() >= split + 4 + length {
            return Ok(request);
        }
    }
}

fn reply(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let text = match status {
        200 => "OK",
        400 => "Bad Request",
        413 => "Payload Too Large",
        422 => "Unprocessable Content",
        _ => "Not Found",
    };
    write!(stream, "HTTP/1.1 {status} {text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len())?;
    stream.write_all(body)
}

fn dashboard_payload(bytes: &[u8]) -> Result<Value, String> {
    let save = Save::from_bytes(bytes).map_err(|error| error.to_string())?;
    let alpha = save.header.version == 105;
    let huffman = HuffmanTree::new();
    let level = 1u8;
    let mut equipment = Vec::new();
    let mut inventory = Vec::new();
    let mut belt = Vec::new();
    let mut stash = Vec::new();
    let mut cube = Vec::new();
    let mut unknown = Vec::new();
    if let Ok(items) = Item::read_player_items(bytes, &huffman, alpha) {
        for (index, item) in items.iter().enumerate() {
            if item.is_residue() || item.code.trim().is_empty() {
                continue;
            }
            let data = item_json(index, item, level);
            let route = if alpha && item.location == 12 {
                AlphaInventoryRoute::Stash
            } else {
                alpha_inventory_route(item, alpha)
            };
            match route {
                AlphaInventoryRoute::Equipment => equipment.push(data),
                AlphaInventoryRoute::Belt => belt.push(data),
                AlphaInventoryRoute::Inventory => inventory.push(data),
                AlphaInventoryRoute::Cube => cube.push(data),
                AlphaInventoryRoute::Stash => stash.push(data),
                AlphaInventoryRoute::Unknown => unknown.push(data),
            }
        }
    }
    Ok(
        json!({"character":{"name":save.header.char_name,"class":d2r_core::save::class_name(save.header.char_class),"level":level,"experience":0,"stats":{},"gold":{}},"all_attributes":[],"skills":[],"waypoints":{"normal":[],"nightmare":[],"hell":[]},"quests":{"normal":[],"nightmare":[],"hell":[]},"equipment":equipment,"inventory":inventory,"unknown":unknown,"belt":belt,"stash":stash,"cube":cube,"mercenary":{"equipped_items":[]}}),
    )
}

fn item_json(index: usize, item: &Item, level: u8) -> Value {
    let ko = format_item(item, "ko", 0, level);
    let en = format_item(item, "en", 0, level);
    json!({"index":index,"code":item.code.trim(),"name_en":item.code.trim(),"name_ko":item.code.trim(),"type":d2r_core::inventory::get_item_category(&item.code),"quality":format!("{:?}", item.header.quality),"x":item.x,"y":item.y,"width":d2r_core::inventory::get_item_size(&item.code).0,"height":d2r_core::inventory::get_item_size(&item.code).1,"is_equipped":item.mode==1,"location":item.location,"properties":properties(&item.properties),"formatted_lines_ko":ko.properties,"formatted_lines_en":en.properties,"formatted_base_ko":ko.base_attributes,"formatted_base_en":en.base_attributes,"set_attributes":[],"runeword_attributes":properties(&item.runeword_attributes),"socketed_items":[]})
}

fn properties(items: &[ItemProperty]) -> Vec<Value> {
    items.iter().map(|item| json!({"stat_id":item.stat_id,"name":item.name,"param":item.param,"raw_value":item.raw_value,"value":item.value})).collect()
}
