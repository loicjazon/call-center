//! Faux serveur HTTP local pour tester les appels réseau sans Internet.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub struct StubServer {
    pub base_url: String,
    /// Requêtes reçues (ligne de requête et en-têtes), dans l'ordre.
    pub requests: Arc<Mutex<Vec<String>>>,
}

/// Lit une requête HTTP (jusqu'à la fin des en-têtes).
fn read_request(stream: &mut impl Read) -> String {
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
    String::from_utf8_lossy(&request).into_owned()
}

/// Sert `connections` connexions, la réponse (statut, corps) étant choisie par `respond`
/// à partir du numéro de la connexion et de la requête reçue.
fn serve_with(
    connections: usize,
    respond: impl Fn(usize, &str) -> (u16, String) + Send + 'static,
) -> StubServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&requests);
    std::thread::spawn(move || {
        for index in 0..connections {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let request = read_request(&mut stream);
            let (status, body) = respond(index, &request);
            log.lock().unwrap().push(request);
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

/// Répond, dans l'ordre, une réponse (statut, corps) par connexion, puis s'arrête.
pub fn serve(responses: Vec<(u16, String)>) -> StubServer {
    let count = responses.len();
    serve_with(count, move |index, _| responses[index].clone())
}

/// Répond selon la requête : la première route dont le motif figure dans la requête, sinon 404.
/// Sert `connections` connexions (utile quand l'ordre d'arrivée n'est pas déterministe).
pub fn serve_routes(routes: Vec<(&'static str, (u16, String))>, connections: usize) -> StubServer {
    serve_with(connections, move |_, request| {
        let line = request.lines().next().unwrap_or_default();
        routes
            .iter()
            .find(|(pattern, _)| line.contains(pattern))
            .map(|(_, answer)| answer.clone())
            .unwrap_or((404, String::new()))
    })
}
