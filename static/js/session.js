const SESSION_PAUSED =  0;
const SESSION_RUNNING = 1;
const SESSION_WAITING = 2;

let client = {};
let session = {clients: {}};
session.status = SESSION_WAITING;

let title = document.getElementById("title");
let timer = document.getElementById("timer");
let buzzer = document.getElementById("buzzer");
let users = document.getElementById("users");
let name = document.getElementById("name");
let admin = document.getElementById("admin");
let start_session = document.getElementById("start_session");
let reset_session = document.getElementById("reset_session");
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
			update_clients();
			break;
		case "Buzzed":
			session.clients[msg.id].buzz = true;
			session.buzzer = msg.id;
			session.status = SESSION_PAUSED;
			clearInterval(session.timer_interval);
			update_clients();
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
	buzzer.addEventListener("click", function () {
		ws.send(JSON.stringify({type: "Buzz"}));
	});
	name.addEventListener("blur", function () {
		ws.send(JSON.stringify({type: "ChangeName", name: name.value}));
	});
	reset_session.addEventListener("click", function () {
		ws.send(JSON.stringify({type: "ResetSession"}));
	});
	resume_session.addEventListener("click", function () {
		ws.send(JSON.stringify({type: "ResumeSession"}));
	});
	session_name.addEventListener("blur", function () {
		ws.send(JSON.stringify({type: "ChangeSession", name: session_name.value, timer: parseInt(session_timer.value)}));
	});
	session_timer.addEventListener("blur", function () {
		ws.send(JSON.stringify({type: "ChangeSession", name: session_name.value, timer: parseInt(session_timer.value)}));
	});
}
