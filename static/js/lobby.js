let button = document.getElementById("new_session");
let name  = document.getElementById("name");
let timer = document.getElementById("timer");

button.addEventListener("click", () => {
    window.location.assign(`/create?name=${name.value}&timer=${timer.value}`);
});
