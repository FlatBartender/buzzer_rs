const SESSION_PAUSED =  0;
const SESSION_RUNNING = 1;
const SESSION_WAITING = 2;

let client = {};
let buzz_sound = new Audio("/static/snd/buzz.ogg");
let session = {clients: {}};
session.status = SESSION_WAITING;

let title = document.getElementById("title");
let timer = document.getElementById("timer");
let buzzer = document.getElementById("buzzer");
let users = document.getElementById("users");
let name = document.getElementById("name");
let admin = document.getElementById("admin");
let start_session = document.getElementById("start_session");
let pause_session = document.getElementById("pause_session");
let reset_session = document.getElementById("reset_session");
let reset_blacklist = document.getElementById("reset_blacklist");
let session_name = document.getElementById("session_name");
let session_timer = document.getElementById("session_timer");

let ws;
if (window.location.protocol.startsWith("https")) {
	ws = new WebSocket(`wss://${window.location.host}${window.location.pathname}/ws`);
} else {
	ws = new WebSocket(`ws://${window.location.host}${window.location.pathname}/ws`);
}

function update_clients() {
	let users = d3.select("#users")
		.selectAll("div")
		.data(Object.values(session.clients))
			.classed("buzzer", function (d) { return d.buzz })
			.classed("fail", function (d) { return d.fail })
			.text(function(d) { return d.name });

	users.enter().append("div")
		.classed("buzzer", function (d) { return d.buzz })
		.classed("fail", function (d) { return d.fail })
		.text(function(d) { return d.name });

	users.exit().remove();
}

ws.addEventListener("message", function (event) {
	let msg = JSON.parse(event.data);

	switch (msg.type) {
		case "ConnectionSuccess":
			client.id = msg.id;
			if (!msg.is_admin) {
				admin.style.display = "none";
			}
			session.timer = msg.timer;
			session.elapsed = msg.elapsed
			session.status = msg.status;
			document.title = `Session ${msg.name}`;
			title.textContent = msg.name;
			timer.textContent = (session.timer - msg.elapsed / 1000).toFixed(1);
			session_name.value = msg.name;
			session_timer.value = msg.timer;
			break;
		case "Connected":
			session.clients[msg.id] = {name: msg.name};
			update_clients();
			break;
		case "ChangedName":
			session.clients[msg.id].name = msg.name;
			update_clients();
			break;
		case "Resumed":
			if (session.clients[session.buzzer]) {
				session.clients[session.buzzer].buzz = undefined;
				session.clients[session.buzzer].fail = true;
			}
			delete session.buzzer;
			session.left = msg.left;
			timer.textContent = (session.left / 1000).toFixed(1);
			session.timer_interval = setInterval(function () {
				session.left -= 100;
				if (session.left >= 0) timer.textContent = (session.left / 1000).toFixed(1);
			}, 100);
			session.status = SESSION_RUNNING;
			update_clients();
			break;
		case "Reset":
			for (client of Object.values(session.clients)) {
				client.buzz = undefined;
				client.fail = undefined;
			}
			delete session.buzzer;
			clearInterval(session.timer_interval);
			session.left = session.timer * 1000;
			timer.textContent = session.timer.toFixed(1);
			session.status = SESSION_WAITING;
			update_clients();
			break;
		case "BlacklistCleared":
			for (client of Object.values(session.clients)) {
				client.buzz = undefined;
				client.fail = undefined;
			}
			delete session.buzzer;
			update_clients();
			break;
		case "Buzzed":
			session.clients[msg.id].buzz = true;
			session.buzzer = msg.id;
			session.status = SESSION_PAUSED;
			buzz_sound.play();
			clearInterval(session.timer_interval);
			update_clients();
			break;
		case "Paused":
			session.status = SESSION_PAUSED;
			clearInterval(session.timer_interval);
			break;
		case "Disconnected":
			delete session.clients[msg.id];
			update_clients();
			break;
		case "Changed":
			session.name = msg.name;
			session.timer = msg.timer;
			document.title = `Session ${msg.name}`;
			title.textContent = msg.name;
			timer.textContent = session.timer.toFixed(1);
			session_name.value = msg.name;
			session_timer.value = msg.timer;
			break;
		case "Closed":
			alert("Session closed");
			break;
	}
	console.log(msg);
});

ws.onopen = function () {
	ws.send(JSON.stringify({type: "Connect"}));
	document.addEventListener('keydown', function(event) {
		console.log(event);
		switch (event.key) {
			case "r":
				ws.send(JSON.stringify({type: "ResetSession"}));
				break;
			case " ":
				switch(session.status) {
					case SESSION_RUNNING:
						ws.send(JSON.stringify({type: "PauseSession"}));
						break;
					case SESSION_WAITING:
					case SESSION_PAUSED:
						ws.send(JSON.stringify({type: "ResumeSession"}));
						break;
				}
				break;
			case "c":
				ws.send(JSON.stringify({type: "ResetBlacklist"}));
				break;
		}
	});
	buzzer.addEventListener("click", function () {
		ws.send(JSON.stringify({type: "Buzz"}));
	});
	buzzer.addEventListener("focus", function (event) {
		this.blur();
	});
	name.addEventListener("blur", function () {
		ws.send(JSON.stringify({type: "ChangeName", name: name.value}));
	});
	name.addEventListener("keydown", function (event) {
		event.stopPropagation();
	});
	reset_session.addEventListener("click", function () {
		ws.send(JSON.stringify({type: "ResetSession"}));
	});
	pause_session.addEventListener("click", function () {
		ws.send(JSON.stringify({type: "PauseSession"}));
	});
	resume_session.addEventListener("click", function () {
		ws.send(JSON.stringify({type: "ResumeSession"}));
	});
	reset_blacklist.addEventListener("click", function () {
		ws.send(JSON.stringify({type: "ResetBlacklist"}));
	});
	session_name.addEventListener("blur", function () {
		ws.send(JSON.stringify({type: "ChangeSession", name: session_name.value, timer: parseInt(session_timer.value)}));
	});
	session_name.addEventListener("keydown", function (event) {
		event.stopPropagation();
	});
	session_timer.addEventListener("blur", function () {
		ws.send(JSON.stringify({type: "ChangeSession", name: session_name.value, timer: parseInt(session_timer.value)}));
	});
	session_timer.addEventListener("keydown", function (event) {
		event.stopPropagation();
	});
}
