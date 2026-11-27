from contextlib import closing, contextmanager
from http.cookiejar import CookieJar
from pathlib import Path
import os
import re
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import HTTPCookieProcessor, HTTPRedirectHandler, Request, build_opener


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, url):
        return None


class Client:
    def __init__(self, base):
        self.base = base
        self.opener = build_opener(NoRedirect(), HTTPCookieProcessor(CookieJar()))

    def request(self, path, expected=200, **data):
        request = Request(self.base + path, data=urlencode(data).encode() if data else None)
        try:
            response = self.opener.open(request, timeout=8)
        except HTTPError as error:
            response = error
        with response:
            body = response.read().decode()
            assert response.code == expected, (path, response.code, expected, body[:200])
            return response.headers.get("Location"), body

    def submit(self, page, path, expected=303, **data):
        _, body = self.request(page)
        token = re.search(r'name="csrf" value="([^"]+)"', body).group(1)
        return self.request(path, expected, csrf=token, **data)

    def login(self, username, password, expected=303):
        return self.submit("/login", "/login", expected, username=username, password=password)[0]


@contextmanager
def server(binary, database):
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    bind = f"127.0.0.1:{port}"
    base = f"http://{bind}"
    env = dict(os.environ, TASKAPP_MODE="local", TASKAPP_BIND=bind, TASKAPP_DB=str(database))
    process = subprocess.Popen([str(binary)], env=env)
    try:
        for _ in range(100):
            if process.poll() is not None:
                raise RuntimeError(f"taskApp exited with {process.returncode}")
            try:
                Client(base).request("/login")
                break
            except URLError:
                time.sleep(0.1)
        else:
            raise RuntimeError("taskApp did not start")
        yield base
    finally:
        process.terminate()
        process.wait(timeout=10)


def check_accounts(base, database):
    admin, other_admin, user = (Client(base) for _ in range(3))
    assert admin.login("admin", "admin") == "/"
    assert other_admin.login("admin", "admin") == "/"
    assert Client(base).request("/usuarios", 303)[0] == "/login"
    temporary = "temporal para pruebas"
    credentials = dict(username="visita", password=temporary, repeat=temporary)
    admin.request("/usuarios", 403, csrf="incorrecto", **credentials)
    assert admin.submit("/usuarios/nuevo", "/usuarios", **credentials)[0] == "/usuarios"
    _, duplicate = admin.submit("/usuarios/nuevo", "/usuarios", 422, **credentials)
    assert "Ese usuario ya existe" in duplicate and temporary not in duplicate

    with closing(sqlite3.connect(database)) as conn, conn:
        user_id, name = conn.execute("SELECT id, name FROM users WHERE username = 'visita'").fetchone()
        assert not name
        conn.execute(
            "INSERT INTO tasks (activity_date, store, supervisor, zone, work, intervention, status, notes, owner_id)"
            " VALUES ('2026-11-27', 'Tienda prueba', 'Supervisor', 'Zona', 'Tarea preservada',"
            " 'Seguimiento', 'Pendiente', '', ?)", (user_id,)
        )
    path = f"/usuarios/{user_id}"
    admin.request("/usuarios/1/eliminar", 403)
    admin.submit(path + "/editar", path + "/editar", username="visita-editada")
    assert user.login("visita-editada", temporary) == "/cuenta/contrasena"
    assert user.request("/cuenta", 303)[0] == "/login"
    password = "definitiva para pruebas"
    user.submit("/cuenta/contrasena", "/cuenta/contrasena", password=password, repeat=password)
    user.request("/usuarios", 403)
    user.submit("/cuenta", path + "/eliminar", 403, username="visita-editada")
    user.submit("/cuenta", "/cuenta", name="Nombre personal")
    assert "Nombre personal" in admin.request("/usuarios")[1]

    reset = "nueva temporal de prueba"
    admin.submit(path + "/restablecer", path + "/restablecer", password=reset, repeat=reset)
    assert user.request("/cuenta", 303)[0] == "/login"
    with closing(sqlite3.connect(database)) as conn, conn:
        conn.execute("UPDATE users SET temporary_until = 0 WHERE id = ?", (user_id,))
    assert Client(base).login("visita-editada", reset, 401) is None
    admin.submit(path + "/eliminar", path + "/eliminar", 422, username="incorrecto")
    admin.submit(path + "/eliminar", path + "/eliminar", username="visita-editada")
    admin.request(path + "/editar", 404)
    assert "Tarea preservada" in admin.request("/historial?usuario=0")[1]
    with closing(sqlite3.connect(database)) as conn, conn:
        assert conn.execute("SELECT owner_id FROM tasks").fetchone() == (None,)
        assert conn.execute(
            "SELECT action FROM user_events WHERE user_id = ? ORDER BY id", (user_id,)
        ).fetchall() == [("crear",), ("editar",), ("cambiar_contrasena",), ("restablecer",), ("eliminar",)]

    rotated = "administrador de prueba"
    admin.submit("/cuenta/contrasena", "/cuenta/contrasena", 422,
                 current="incorrecta", password=rotated, repeat=rotated)
    assert admin.submit("/cuenta/contrasena", "/cuenta/contrasena",
                        current="admin", password=rotated, repeat=rotated)[0] == "/cuenta"
    assert "Contraseña actualizada" in admin.request("/cuenta")[1]
    assert other_admin.request("/usuarios", 303)[0] == "/login"
    return rotated


if __name__ == "__main__":
    binary = Path(sys.argv[1]).resolve()
    with tempfile.TemporaryDirectory(prefix="taskapp-test-") as directory:
        database = Path(directory) / "test.sqlite"
        with server(binary, database) as base:
            rotated = check_accounts(base, database)
        with server(binary, database) as base:
            assert Client(base).login("admin", "admin", 401) is None
            assert Client(base).login("admin", rotated) == "/"
    print("Pruebas HTTP de cuentas y persistencia del administrador completadas.")
