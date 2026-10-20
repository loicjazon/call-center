//! Faux serveur HTTP local pour tester les appels réseau sans Internet.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub struct StubServer {
    pub base_url: String,
    /// Requêtes reçues (ligne de requête et en-têtes), dans l'ordre.
    pub requests: Arc<Mutex<Vec<String>>>,
}

/// Répond, dans l'ordre, une réponse (statut, corps) par connexion, puis s'arrête.
pub fn serve(responses: Vec<(u16, String)>) -> StubServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&requests);
    std::thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut chunk = [0u8; 4096];
            let mut request = Vec::new();
            loop {
                let read = stream.read(&mut chunk).unwrap_or(0);
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            log.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&request).into_owned());
            let response = format!(
                "HTTP/1.1 {status} Stub\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    StubServer {
        base_url: format!("http://127.0.0.1:{port}"),
        requests,
    }
}
