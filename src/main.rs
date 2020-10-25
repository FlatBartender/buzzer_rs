use std::collections::HashMap;
use actix::prelude::*;
use actix_files::Files;
use actix_web::{get, web, App, HttpResponse, HttpServer, HttpRequest, Responder, http};
use actix_web_actors::ws;
use serde::{Serialize, Deserialize};
use serde_repr::Serialize_repr;
use serde_json;
use tera::Tera;
use rand::random;
use log::*;
use pretty_env_logger;

type SessionId = u64;
type UserId = u64;

use std::time::{Duration, Instant};
use std::collections::HashSet;
use std::sync::Mutex;

#[derive(Message, Clone, Serialize)]
#[rtype(result = "()")]
#[serde(tag = "type")]
pub enum SessionMessage {
    // Timers are reset to their initial durations, buzz blacklist is cleared
    Reset,
    Closed,
    // The u64 here is used to keep clock drift in check. It's the number of milliseconds left.
    Resumed{left: u64},
    Changed{name: String, timer: u64}, // Timer is in seconds
    Connected{name: String, id: UserId},
    ChangedName{name: String, id: UserId},
    Disconnected{id: UserId},
    ConnectionSuccess{id: UserId, is_admin: bool, name: String, timer: u64, elapsed: u64, status: SessionStatus},
    Buzzed{id: UserId},
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum RawClientMessage {
    Buzz,
    ChangeName{name: String},
    ChangeSession{name: String, timer: u64}, // Timer is in seconds
    CloseSession,
    ResumeSession,
    ResetSession,
    Disconnected,
    Connect,
}

#[derive(Message)]
#[rtype(result = "()")]
pub enum ClientMessage {
    // Trigger the buzz 
    Buzz{from: UserId},
    ChangeName{from: UserId, name: String},
    ChangeSession{from: UserId, name: String, timer: u64}, // Timer is in seconds
    CloseSession{from: UserId},
    ResumeSession{from: UserId},
    ResetSession{from: UserId},
    Disconnected{from: UserId},
    Connect{addr: Addr<Client>},
}

#[derive(PartialEq, Clone, Serialize_repr)]
#[repr(u8)]
pub enum SessionStatus {
    Paused = 0,
    Running = 1,
    Waiting = 2,
}

// Represents a Session, which has exactly one admin and any number of clients.
pub struct Session {
    name: String,
    timer: Duration,
    timer_handle: Option<SpawnHandle>,
    timer_last_start: Instant,
    elapsed: Duration,
    blacklist: HashSet<UserId>, // Contains the clients that have already buzzed
    // Contains both the Admin and the Clients
    admin: UserId,
    last_uid: UserId,
    status: SessionStatus,
    clients: HashMap<UserId, (Addr<Client>, String)>,
}

impl Session {
    fn get_next_uid(&mut self) -> UserId {
        self.last_uid += 1;
        self.last_uid
    }

    fn broadcast(&self, msg: SessionMessage) {
        for sub in self.clients.values() {
            sub.0.do_send(msg.clone());
        }
    }
}

impl Session {
    pub fn reset(&mut self) {
        self.blacklist.clear();
        self.elapsed = Duration::from_secs(0);
        self.status = SessionStatus::Waiting;
        self.broadcast(SessionMessage::Reset);
    }
}

impl Actor for Session {
    type Context = Context<Self>;
}

impl Handler<ClientMessage> for Session {
    type Result = ();
    fn handle(&mut self, msg: ClientMessage, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            ClientMessage::Buzz{from} =>  {
                if self.status == SessionStatus::Running && !self.blacklist.contains(&from) {
                    self.blacklist.insert(from);
                    if let Some(handle) = self.timer_handle {
                        ctx.cancel_future(handle);
                    }
                    let elapsed = Instant::now() - self.timer_last_start;
                    self.elapsed += elapsed;
                    self.status = SessionStatus::Paused;
                    self.broadcast(SessionMessage::Buzzed{id: from});
                }
            },
            ClientMessage::ChangeName{from, name} => {
                self.clients.get_mut(&from).unwrap().1 = name.clone();
                self.broadcast(SessionMessage::ChangedName{id: from, name});
            }
            ClientMessage::ChangeSession{from, name, timer} => {
                if self.status == SessionStatus::Waiting && self.admin == from {
                    self.timer = Duration::from_secs(timer.clone());
                    self.name = name.clone();
                    self.broadcast(SessionMessage::Changed{name, timer});
                }
            },
            ClientMessage::CloseSession{from} => {
                if self.admin == from {
                    self.broadcast(SessionMessage::Closed);
                    ctx.stop();
                }
            },
            ClientMessage::ResumeSession{from} => {
                if self.admin == from && self.status != SessionStatus::Running {
                    self.status = SessionStatus::Running;
                    self.broadcast(SessionMessage::Resumed{left: (self.timer - self.elapsed).as_millis() as u64});
                    self.timer_last_start = Instant::now();
                    self.timer_handle = Some(ctx.run_later(
                        actix::clock::Duration::from_millis((self.timer.as_millis() - self.elapsed.as_millis()) as u64),
                        |s, _| s.reset()
                    ));
                }
            },
            ClientMessage::ResetSession{from} => {
                if self.admin == from {
                    if let Some(handle) = self.timer_handle {
                        ctx.cancel_future(handle);
                    }
                    self.reset();
                }
            },
            ClientMessage::Disconnected{from} => {
                self.clients.remove(&from);
                if self.admin == from {
                    self.broadcast(SessionMessage::Closed);
                    //ctx.stop();
                } else {
                    self.broadcast(SessionMessage::Disconnected{id: from});
                }
            },
            ClientMessage::Connect{addr} => {
                let id = self.get_next_uid();
                if self.clients.is_empty() {
                    self.admin = id;
                }
                addr.do_send(SessionMessage::ConnectionSuccess{
                    id,
                    is_admin: self.admin == id,
                    name: self.name.clone(),
                    timer: self.timer.as_secs(),
                    status: self.status.clone(),
                    elapsed: self.elapsed.as_millis() as u64,
                });
                self.clients.iter().for_each(|(id, (_, name))| {
                    addr.do_send(SessionMessage::Connected{name: name.clone(), id: id.clone()});
                });
                let name = format!("user{}", id);
                self.clients.insert(id.clone(), (addr, name.clone()));
                self.broadcast(SessionMessage::Connected{name, id: id.clone()});
            }
        }
    }
}

// Represents a Client, which participates in the session and can send a buzz
pub struct Client {
    id: UserId,
    session: Addr<Session>,
}

impl Actor for Client {
    type Context = ws::WebsocketContext<Self>;
}

impl Handler<SessionMessage> for Client {
    type Result = ();

    fn handle(&mut self, msg: SessionMessage, ctx: &mut Self::Context) -> Self::Result {
        if let SessionMessage::ConnectionSuccess{id, ..} = msg {
            self.id = id;
        }

        let val = match serde_json::to_string(&msg) {
            Err(err) => {
                error!("Failed to serialize SessionMessage: {}", err);
                return;
            },
            Ok(val) => val,
        };
        ctx.text(val);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Client {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Err(_) => {
                self.session.do_send(ClientMessage::Disconnected{from: self.id});
                ctx.stop();
                return;
            },
            Ok(msg) => msg,
        };

        match msg {
            ws::Message::Ping(msg) => ctx.pong(&msg),
            ws::Message::Text(msg) => {
                let msg: RawClientMessage = match serde_json::from_str(msg.as_ref()) {
                    Err(err) => {
                        error!("Received faulty message: {}", err);
                        return;
                    },
                    Ok(msg) => msg
                };
                let from = self.id;
                let msg = match msg {
                    RawClientMessage::Buzz => ClientMessage::Buzz{from},
                    RawClientMessage::ChangeName{name} => ClientMessage::ChangeName{from, name},
                    RawClientMessage::ChangeSession{name, timer} => ClientMessage::ChangeSession{from, name, timer},
                    RawClientMessage::CloseSession => ClientMessage::CloseSession{from},
                    RawClientMessage::ResumeSession => ClientMessage::ResumeSession{from},
                    RawClientMessage::ResetSession => ClientMessage::ResetSession{from},
                    RawClientMessage::Disconnected => ClientMessage::Disconnected{from},
                    RawClientMessage::Connect => ClientMessage::Connect{addr: ctx.address().clone()},
                };
                self.session.do_send(msg);
            },
            ws::Message::Binary(_) => error!("Unexpected binary"),
            ws::Message::Pong(_) => {},
            ws::Message::Close(reason) => {
                ctx.close(reason);
                self.session.do_send(ClientMessage::Disconnected{from: self.id});
            },
            ws::Message::Continuation(_) => error!("Unexpected continuation"),
            ws::Message::Nop => (),
        }
    }
}

#[derive(Default)]
struct BuzzerMainState {
    sessions: HashMap<SessionId, Addr<Session>>,
}


#[get("/")]
async fn lobby(tera: web::Data<Tera>) -> impl Responder {
    match tera.render("index.html", &tera::Context::new()) {
        Err(err) => {
            error!("Failed to render template: {}", err);
            HttpResponse::BadRequest().finish()
        },
        Ok(body) => HttpResponse::Ok().body(body)
    }
}

#[derive(Deserialize)]
struct CreateQuery {
    name: String,
    timer: u64,
}

#[get("/create")]
async fn create(query: web::Query<CreateQuery>, data: web::Data<Mutex<BuzzerMainState>>) -> impl Responder {
    let s = Session {
        name: query.name.clone(),
        timer: Duration::from_secs(query.timer),
        timer_handle: None,
        timer_last_start: Instant::now(),
        elapsed: Duration::from_secs(0),
        blacklist: HashSet::new(),
        admin: 0,
        last_uid: 0,
        status: SessionStatus::Waiting,
        clients: HashMap::new(),
    };

    let mut lock = data.lock().unwrap();
    let mut session_id = random();
    while lock.sessions.contains_key(&session_id) {
        session_id = random();
    }

    let addr = s.start();
    lock.sessions.insert(session_id, addr);
    HttpResponse::Found().header(http::header::LOCATION, format!("/session/{:x}", session_id)).finish()
}

#[get("/session/{session_id}")]
async fn session(web::Path(session_id): web::Path<String>, tera: web::Data<Tera>) -> impl Responder {
    let session_id = match SessionId::from_str_radix(session_id.as_ref(), 16) {
        Err(err) => {
            error!("Unable to parse session ID: {}", err);
            return HttpResponse::NotFound().body("Not Found");
        },
        Ok(id) => id
    };
    
    let mut context = tera::Context::new();
    context.insert("id", &session_id);
    context.insert("id_display", &format!("{:x}", session_id));
    match tera.render("session.html", &context) {
        Err(err) => {
            error!("Failed to render template: {}", err);
            HttpResponse::BadRequest().finish()
        },
        Ok(body) => HttpResponse::Ok().body(body)
    }
}

#[get("/session/{session_id}/ws")]
async fn session_ws(req: HttpRequest, stream: web::Payload, web::Path(session_id): web::Path<String>, data: web::Data<Mutex<BuzzerMainState>>) -> Result<HttpResponse, actix_web::Error> {
    let session_id = match SessionId::from_str_radix(session_id.as_ref(), 16) {
        Err(err) => {
            error!("Unable to parse session ID: {}", err);
            return Ok(HttpResponse::NotFound().body("Not Found"));
        },
        Ok(id) => id
    };
 
    let lock = data.lock().unwrap();
    let s = match lock.sessions.get(&session_id) {
        None => return Ok(HttpResponse::NotFound().finish()),
        Some(s) => s,
    };
    let client = Client {
        id: 0,
        session: s.clone(),
    };

    ws::start(client, &req, stream)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    pretty_env_logger::init();

    let buzzer_state = web::Data::new(Mutex::new(BuzzerMainState::default()));
    let tera = match Tera::new("templates/**/*.html") {
        Ok(t) => t,
        Err(err) => {
            error!("Parsing error: {}", err);
            std::process::exit(1);
        }
    };
    let tera = web::Data::new(tera);

    HttpServer::new(move || {
        App::new()
            .app_data(rand::thread_rng())
            .app_data(buzzer_state.clone())
            .app_data(tera.clone())
            .service(session)
            .service(session_ws)
            .service(create)
            .service(lobby)
            .service(Files::new("/static", "static").prefer_utf8(true))
    })
    .bind("127.0.0.1:3010")?
        .run()
        .await
}
