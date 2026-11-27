const DAY = 86_400_000;
const HOURS_8 = DAY / 3;
const KEYS = {
  date: 'Fecha de actividad', store: 'Tienda', supervisor: 'Supervisor General',
  zone: 'Zona', work: 'Trabajo realizado', intervention: 'Tipo de intervención',
  status: 'Estado', notes: 'Observaciones',
};
const ADD = '__new__';
const available = state => Boolean(state.snapshot && Date.now() < state.expires);
const textValue = (values, field) => values[field] === ADD ? values[field + '_new'] : values[field];
const toValues = task => Object.fromEntries(Object.keys(KEYS).map(
  field => [field, task[field === 'date' ? 'activity_date' : field]]
));
const selectedPath = new URL(location.href);

function openDb() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('taskapp-device', 1);
    request.onupgradeneeded = () => request.result.createObjectStore('workspace');
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function workspace(db, change) {
  return new Promise((resolve, reject) => {
    const transaction = db.transaction('workspace', change ? 'readwrite' : 'readonly');
    const store = transaction.objectStore('workspace');
    const request = store.get('current');
    let value;
    request.onsuccess = () => {
      value = request.result || { queue: [] };
      if (change) {
        change(value);
        store.put(value, 'current');
      }
    };
    transaction.oncomplete = () => resolve(value);
    transaction.onabort = () => reject(transaction.error);
  });
}

export async function chartSnapshot() {
  const db = await openDb();
  try {
    const state = await workspace(db);
    return available(state) ? { ...state.snapshot, expires: state.expires } : null;
  } finally {
    db.close();
  }
}

function asuncionToday() {
  const parts = Object.fromEntries(new Intl.DateTimeFormat('en', {
    timeZone: 'America/Asuncion', year: 'numeric', month: '2-digit', day: '2-digit',
  }).formatToParts().map(({ type, value }) => [type, value]));
  return parts.year + '-' + parts.month + '-' + parts.day;
}

function element(tag, text, className) {
  const node = document.createElement(tag);
  if (text) node.textContent = text;
  if (className) node.className = className;
  return node;
}

function button(label, action, style = 'quiet') {
  const node = element('button', label, style);
  node.type = 'button';
  node.addEventListener('click', action);
  return node;
}

export async function startOffline(showConnection) {
  const message = document.querySelector('#app-message');
  const device = document.querySelector('#device');
  const deviceLogin = document.querySelector('#device-login');
  const list = document.querySelector('#device-list');
  const status = document.querySelector('#device-state');
  const form = document.querySelector('.task-form');
  const signout = document.querySelector('.signout');
  const isLogin = location.pathname === '/login' && !device;
  const isPrivate = Boolean(signout || device);
  if (!isPrivate && !isLogin) return;

  const announce = (text, style = 'success') => {
    message.textContent = text;
    message.className = 'notice ' + style;
    message.hidden = !text;
  };
  let db;
  let state;
  try {
    db = await openDb();
    state = await workspace(db);
  } catch {
    if (status) status.textContent = 'El almacenamiento local no está disponible en este navegador.';
    return;
  }
  const channel = typeof BroadcastChannel === 'function' ? new BroadcastChannel('taskapp-device') : null;
  let editing = null;
  let running;
  let inLogout = false;
  let invalidSession = false;

  async function change(update) {
    state = await workspace(db, update);
    channel?.postMessage('changed');
  }

  function loadForm(values, context) {
    editing = context;
    form.elements.date.value = values.date || asuncionToday();
    form.elements.notes.value = values.notes || '';
    for (const field of form.querySelectorAll('[data-choice]')) {
      const kind = field.querySelector('select').name;
      field.dispatchEvent(new CustomEvent('taskapp-values', { detail: {
        suggestions: state.snapshot.catalog.filter(option => option.kind === kind).map(option => option.label),
        queued: state.queue.filter(entry => entry.values[kind] === ADD).map(entry => entry.values[kind + '_new']),
        selected: values[kind] === ADD ? '' : values[kind],
        newValue: values[kind] === ADD ? values[kind + '_new'] : '',
      }}));
    }
    form.querySelector('[data-submit-label]').textContent = context ? 'Guardar cambios' : 'Guardar tarea';
    document.querySelector('#device-form-title').textContent = context ? 'Editar tarea' : 'Nueva tarea';
    form.querySelector('.device-compare')?.remove();
    if (context?.remote) {
      const compare = element('div', '', 'device-compare conflict');
      compare.append(element('h2', 'Versión guardada'));
      const fields = element('dl');
      for (const [field, label] of Object.entries(KEYS)) {
        fields.append(element('dt', label), element('dd', toValues(context.remote)[field]));
      }
      compare.append(element('p', 'Tu edición está en el formulario. Ajusta los campos que quieras conservar.'), fields);
      form.prepend(compare);
    }
    form.hidden = false;
    form.elements.date.focus();
    form.scrollIntoView({ block: 'start' });
  }

  function closeForm() {
    form?.querySelector('.device-compare')?.remove();
    if (form && device) form.hidden = true;
    editing = null;
  }

  function showNew() {
    loadForm({
      date: selectedPath.searchParams.get('fecha') || asuncionToday(),
      store: selectedPath.searchParams.get('tienda') || '',
      supervisor: 'FLORENCIA SILVERO', zone: 'INTERIOR',
    }, null);
  }

  function render() {
    const count = document.querySelector('[data-pending-count]');
    if (count) {
      count.hidden = !state.queue.length;
      count.textContent = String(state.queue.length);
      count.setAttribute('aria-label', state.queue.length + ' cambios pendientes');
    }
    if (!device) return;
    const unlocked = available(state);
    device.hidden = !unlocked;
    deviceLogin.hidden = unlocked || !navigator.onLine;
    if (!unlocked) {
      closeForm();
      list.replaceChildren();
      status.textContent = state.snapshot || state.queue.length
        ? 'Inicia sesión con conexión para recuperar tus tareas y cambios pendientes.'
        : 'Abre taskApp con conexión e inicia sesión para preparar el acceso sin conexión.';
      return;
    }
    status.textContent = 'Copia de los últimos 30 días · ' + state.queue.length + ' cambios pendientes';
    list.replaceChildren();
    const pendingIds = new Set(state.queue.map(entry => entry.id));
    for (const entry of state.queue) {
      const section = element('article', '', 'panel device-record');
      section.append(element('h3', textValue(entry.values, 'work')));
      const summary = textValue(entry.values, 'store') + ' · ' + entry.values.date;
      const note = entry.state === 'conflict' ? 'Requiere revisión'
        : entry.state === 'invalid' ? 'Requiere corrección' : 'Pendiente de sincronizar';
      section.append(element('p', summary, 'record-meta'), element('p', note, 'status'));
      if (entry.errors) section.append(element('p', Object.values(entry.errors).join(' '), 'field-error'));
      if (entry.state !== 'sending') {
        const context = { id: entry.id, revision: entry.remote?.revision ?? entry.revision, operation: entry.operation, remote: entry.remote };
        section.append(button(entry.remote ? 'Revisar mi edición' : 'Editar', () => loadForm(entry.values, context)));
        section.append(button(entry.remote ? 'Usar versión guardada' : 'Descartar', async () => {
          if (!window.confirm('¿Descartar la edición de este dispositivo?')) return;
          await change(current => {
            const pending = current.queue.find(item => item.operation === entry.operation);
            if (pending?.state === 'sending') return;
            current.queue = current.queue.filter(item => item.operation !== entry.operation);
            if (entry.remote && current.snapshot) {
              current.snapshot.tasks = current.snapshot.tasks.filter(task => task.id !== entry.id);
              if (entry.remote.activity_date >= current.snapshot.from && entry.remote.activity_date <= current.snapshot.date) {
                current.snapshot.tasks.unshift(entry.remote);
              }
            }
          });
          closeForm();
          render();
        }));
      }
      list.append(section);
    }
    const saved = state.snapshot.tasks.filter(task => !pendingIds.has(task.id));
    if (!saved.length && !state.queue.length) {
      const empty = element('div', '', 'paper-card empty-state');
      empty.append(element('h2', 'Aún no hay tareas en este dispositivo'),
        element('p', 'Las tareas de los últimos 30 días se guardan aquí para trabajar sin conexión.'));
      list.append(empty);
    }
    if (saved.length) list.append(element('h2', 'Guardadas en el servidor', 'device-heading'));
    for (const task of saved) {
      const section = element('article', '', 'panel device-record');
      section.append(element('h3', task.work), element('p', task.store + ' · ' + task.activity_date, 'record-meta'));
      section.append(button('Editar', () => loadForm(toValues(task), { id: task.id, revision: task.revision })));
      list.append(section);
    }
  }

  async function snapshot() {
    let response;
    try {
      response = await fetch('/api/offline', { cache: 'no-store' });
    } catch (error) {
      showConnection(false);
      throw error;
    }
    showConnection(true);
    if (response.status === 401) {
      invalidSession = true;
      if (state.expires) await change(current => { current.expires = 0; });
      return null;
    }
    if (!response.ok) throw new Error('No se pudo actualizar la copia.');
    invalidSession = false;
    const { csrf, ...data } = await response.json();
    const first = new Date(data.date + 'T00:00:00Z');
    first.setUTCDate(first.getUTCDate() - 29);
    data.from = first.toISOString().slice(0, 10);
    await change(current => {
      current.snapshot = data;
      current.expires = Date.now() + HOURS_8;
    });
    return csrf;
  }

  async function synchronize() {
    const results = new Map();
    if (inLogout || !navigator.onLine) return results;
    const csrf = await snapshot();
    if (!csrf) {
      if (state.queue.length && !device) announce('Inicia sesión para sincronizar tus cambios pendientes.', 'error-notice');
      return results;
    }
    if (!state.queue.length) return results;
    for (const candidate of state.queue) {
      if (candidate.state === 'conflict' || candidate.state === 'invalid') continue;
      await change(current => {
        const entry = current.queue.find(item => item.operation === candidate.operation);
        if (entry?.state === 'queued') entry.state = 'sending';
      });
      const entry = state.queue.find(item => item.operation === candidate.operation);
      if (entry?.state !== 'sending') continue;
      const response = await fetch('/api/tareas/sync', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf },
        body: JSON.stringify({ operation: entry.operation, id: entry.id, revision: entry.revision, values: entry.values }),
      });
      if (response.ok) {
        const confirmed = await response.json();
        results.set(entry.operation, confirmed);
        await change(current => {
          current.queue = current.queue.filter(item => item.operation !== entry.operation);
          const saved = { ...toValuesFromEntry(entry), id: confirmed.id, revision: confirmed.revision };
          if (current.snapshot) {
            current.snapshot.tasks = current.snapshot.tasks.filter(task => task.id !== saved.id);
            if (saved.activity_date >= current.snapshot.from && saved.activity_date <= current.snapshot.date) {
              current.snapshot.tasks.unshift(saved);
            }
          }
        });
      } else if ([409, 422, 404].includes(response.status)) {
        const details = response.status === 404 ? { errors: { task: 'La tarea ya no está disponible.' } } : await response.json();
        const stateName = response.status === 409 ? 'conflict' : 'invalid';
        await change(current => {
          const pending = current.queue.find(item => item.operation === entry.operation);
          if (pending?.state === 'sending') Object.assign(pending, { state: stateName, remote: details.task, errors: details.errors });
        });
        results.set(entry.operation, { state: stateName });
      } else {
        if (response.status === 401) {
          invalidSession = true;
          await change(current => { current.expires = 0; });
        }
        break;
      }
    }
    if (results.size) await snapshot().catch(() => {});
    return results;
  }

  function toValuesFromEntry(entry) {
    return Object.fromEntries(Object.keys(KEYS).map(field => [field === 'date' ? 'activity_date' : field, textValue(entry.values, field)]));
  }

  function withLock(callback) {
    return navigator.locks ? navigator.locks.request('taskapp-sync', callback) : callback();
  }

  function runSync() {
    if (running) return running;
    running = withLock(synchronize)
      .catch(() => new Map())
      .finally(() => { running = null; render(); });
    return running;
  }

  if (form) form.addEventListener('submit', async event => {
    if (inLogout) { event.preventDefault(); return; }
    if (!device && !available(state)) {
      if (navigator.onLine && !invalidSession) return;
      event.preventDefault();
      announce('Conéctate e inicia sesión para preparar el guardado local.', 'error-notice');
      return;
    }
    if (!available(state)) {
      event.preventDefault();
      render();
      return;
    }
    event.preventDefault();
    const values = Object.fromEntries(new FormData(form));
    delete values.csrf;
    const id = device ? editing?.id : (Number(location.pathname.split('/').at(-1)) || undefined);
    const revision = device ? editing?.revision : (Number(values.revision) || undefined);
    delete values.revision;
    const operation = crypto.randomUUID();
    const previous = device ? editing?.operation : undefined;
    const submit = event.submitter || form.querySelector('button[type=submit]');
    submit.disabled = true;
    try {
      state = await workspace(db);
      if (!available(state)) {
        announce('La sesión local venció. Inicia sesión con conexión.', 'error-notice');
        return;
      }
      const entry = { operation, id, revision, values, state: 'queued' };
      await change(current => {
        if (!available(current)) return;
        const older = current.queue.find(item => (id && item.id === id) || item.operation === previous);
        if (previous && (!older || older.operation !== previous || older.state === 'sending')) return;
        if (!previous && older) return;
        current.queue = current.queue.filter(item => item.operation !== previous);
        current.queue.push(entry);
      });
      if (!state.queue.some(item => item.operation === operation)) {
        announce('Ya hay un cambio para esta tarea. Revísalo en Dispositivo.', 'error-notice');
        return;
      }
      // If a refresh already captured the old queue, run once more for this entry.
      const first = await runSync();
      const result = first.get(operation) || (await runSync()).get(operation);
      const awaiting = state.queue.find(item => item.operation === operation);
      if (result?.state || awaiting?.state === 'invalid' || awaiting?.state === 'conflict') {
        announce('Revisa el cambio desde Dispositivo.', 'error-notice');
        closeForm();
        render();
        return;
      }
      if (!device) {
        if (result?.id) {
          location.assign(id ? '/historial?edited=true' : '/tareas/nueva?saved=' + result.id);
        } else {
          const keep = new URLSearchParams({ fecha: values.date, tienda: textValue(values, 'store'), guardada: 'local' });
          location.assign('/sin-conexion?' + keep.toString());
        }
      } else {
        closeForm();
        render();
        announce(result?.id ? 'Tarea guardada.' : 'Cambio guardado en este dispositivo. Pendiente de sincronizar.');
        if (!id && !previous) loadForm({ date: values.date, store: textValue(values, 'store'),
          supervisor: 'FLORENCIA SILVERO', zone: 'INTERIOR' }, null);
      }
    } catch {
      announce('No se pudo guardar en el dispositivo. Los campos siguen en el formulario.', 'error-notice');
    } finally {
      submit.disabled = false;
    }
  });

  if (signout) signout.addEventListener('submit', async event => {
    event.preventDefault();
    if (inLogout) return;
    inLogout = true;
    try {
      await withLock(async () => {
        state = await workspace(db);
        if (state.queue.length && !window.confirm('Hay cambios pendientes. ¿Descartarlos en este dispositivo y cerrar sesión?')) return;
        const response = await fetch('/salir', { method: 'POST', body: new URLSearchParams(new FormData(signout)) });
        if (!response.ok || !response.redirected || new URL(response.url).pathname !== '/login') throw new Error();
        await change(current => {
          delete current.snapshot;
          delete current.expires;
          current.queue = [];
        });
        channel?.postMessage('logout');
        location.assign('/login');
      });
    } catch {
      announce('Conéctate para cerrar sesión. Tus cambios se mantienen en el dispositivo.', 'error-notice');
    } finally {
      inLogout = false;
    }
  });

  if (isLogin) {
    if (state.queue.length) announce('Hay cambios pendientes en este dispositivo. Inicia sesión para sincronizarlos.');
    return;
  }

  document.querySelector('#device-new')?.addEventListener('click', showNew);
  document.querySelector('#device-cancel')?.addEventListener('click', () => {
    closeForm();
    document.querySelector('#device-new').focus();
  });
  document.querySelector('#sync-now')?.addEventListener('click', async () => {
    await runSync();
    announce(!navigator.onLine ? 'Sin conexión. Tus cambios se mantienen en este dispositivo.'
      : (state.queue.length ? 'Quedan cambios pendientes. Revisa la lista.' : 'No hay cambios pendientes.'));
  });
  channel?.addEventListener('message', async event => {
    if (event.data === 'logout') {
      location.assign('/login');
      return;
    }
    state = await workspace(db);
    render();
  });
  window.addEventListener('online', runSync);
  setInterval(() => {
    if (navigator.onLine && !document.hidden && !invalidSession
      && state.queue.some(entry => entry.state === 'queued' || entry.state === 'sending')) runSync();
  }, 10_000);
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) { render(); runSync(); }
  });
  window.addEventListener('pageshow', async event => {
    if (!event.persisted) return;
    state = await workspace(db);
    render();
    runSync();
  });
  if (device) {
    window.addEventListener('offline', render);
    setInterval(renderExpiry, 60_000);
    if (selectedPath.searchParams.has('guardada')) announce('Cambio guardado en este dispositivo. Pendiente de sincronizar.');
  }
  function renderExpiry() { if (!available(state)) render(); }
  render();
  if (device && available(state)) {
    const match = location.pathname.match(/^\/tareas\/(\d+)$/);
    const task = match && state.snapshot.tasks.find(item => item.id === Number(match[1]));
    if (task) {
      const pending = state.queue.find(entry => entry.id === task.id);
      if (!pending) loadForm(toValues(task), { id: task.id, revision: task.revision });
    } else if (location.pathname === '/tareas/nueva') showNew();
  }
  runSync();
}
