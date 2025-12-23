use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use futures_util::StreamExt;
use tokio::{net::{TcpListener, TcpStream}, sync::{Mutex, mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel}}, time::Instant};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use uuid::Uuid;
use serde::{Deserialize,Serialize};

type Tx = UnboundedSender<Message>;
type Rx =UnboundedReceiver<Message>;
type RoomType=Arc<Mutex<HashMap<String,RoomDs>>>;
const MAX_ROOM_SIZE:usize=15;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    #[serde(rename = "hello")]
    Hello { username: String },

    #[serde(rename = "create_room")]
    CreateRoom,

    #[serde(rename = "join_room")]
    JoinRoom { room_id: String },

    #[serde(rename = "message")]
    Message { text: String },
}

// Server → Client
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ServerMessage {
    #[serde(rename = "welcome")]
    Welcome { user_id: String },

    #[serde(rename = "room_created")]
    RoomCreated { room_id: String },

    #[serde(rename = "joined_room")]
    JoinedRoom { room_id: String },

    #[serde(rename = "room_message")]
    RoomMessage {
        room_id: String,
        from: String,
        text: String,
    },

    #[serde(rename = "error")]
    Error { message: String },
}


struct RoomDs{
    id:String,
    is_open:bool,
    last_active:Instant,
    created_at:Instant,
    clients:Vec<Clients>,
}

#[derive(Debug,Clone)]
struct Clients{
    id:Uuid,
    username:String,
}

#[tokio::main]
async fn main()->Result<()>{
    let addr ="127.0.0.1:8080";
    let listener = TcpListener::bind(addr).await?;
    println!("Server started at : {}",addr);
    let roomtype:RoomType =Arc::new(Mutex::new(HashMap::new()));
    loop {
        let (stream,peer) = listener.accept().await?;
        println!("New connection at :{}",peer);
        let roomtype = roomtype.clone();
        tokio::spawn(async move{
            if let Err(e)=handle_client(stream,roomtype).await{
                eprintln!("Error is :{}",e);
            }
        });
    }
}

//this is where the room logic is ---------->
async fn create_room(roomtype:RoomType)->String{
    let id =Uuid::new_v4().to_string();
    let now = Instant::now();
    let mut roomtype = roomtype.lock().await;
    let room = RoomDs{
        id:id.clone(),
        is_open:true,
        last_active:now,
        created_at:now,
        clients:Vec::new(),
    };
    roomtype.insert(id.clone(), room);
    id
}

async fn join_room(roomtype:RoomType,room_id:&str,client:Clients)->Result<(),&'static str>{
    let mut roomtype = roomtype.lock().await;
    let room = roomtype.get_mut(room_id).ok_or("Room not found")?;
    if !room.is_open{
        return Err("Room is closed");
    }
    if room.clients.len()>=MAX_ROOM_SIZE{
        return Err("Room is full");
    }
    room.clients.push(client);
    room.last_active=Instant::now();
    Ok(())
}

async fn leave_room(roomtype:RoomType,room_id:&str,user_id:&Uuid){
    let mut roomtype = roomtype.lock().await;
    if let Some(room)=roomtype.get_mut(room_id){
        room.clients.retain(|c|&c.id != user_id);
        room.last_active=Instant::now();
    }
}

async fn cleanup_rooms(roomtype:RoomType){
    let mut roomtype = roomtype.lock().await;
    roomtype.retain(|_,room|!room.clients.is_empty());
}


