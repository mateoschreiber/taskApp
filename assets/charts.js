import { chartSnapshot } from '/offline.js';

const panel = document.querySelector('#charts');
const form = document.querySelector('#chart-range');
const fromInput = form.elements.desde;
const throughInput = form.elements.hasta;
const status = document.querySelector('#chart-status');
const grid = document.querySelector('#chart-grid');
const summary = document.querySelector('#chart-summary');
const account = Number(document.querySelector("[data-account-id]")?.dataset.accountId) || null;
const owner = form.elements.usuario;
const pdf = document.querySelector('#chart-pdf');
const dialog = document.querySelector('#chart-detail');
const detailRows = document.querySelector('#chart-detail-rows');
const day = 86_400_000;
const iso = date => date.toISOString().slice(0, 10);
const short = date => date.slice(8, 10) + '/' + date.slice(5, 7);
const printable = date => short(date) + '/' + date.slice(0, 4);
const asuncion = new Intl.DateTimeFormat('es-PY', {
  timeZone: 'America/Asuncion', dateStyle: 'short', timeStyle: 'short',
});
const compare = (a, b) => a < b ? -1 : Number(a > b);
const groupFields = [
  ['store', 'Tareas por tienda'],
  ['status', 'Distribución por estado'],
  ['work', 'Trabajos más frecuentes'],
  ['intervention', 'Tipos de intervención'],
];
let generation = 0;
let expiryTimer;
const snapshotMode = panel.dataset.offline === 'true';
let controller;
const initial = new URLSearchParams(location.search);
const hasRange = ['desde', 'hasta'].every(field => /^\d{4}-\d{2}-\d{2}$/.test(initial.get(field) ?? ''));
if (hasRange) {
  fromInput.value = initial.get('desde');
  throughInput.value = initial.get('hasta');
  if (owner) owner.value = initial.get('usuario') || '';
}

function rangeParams(from, through) {
  const search = new URLSearchParams({ desde: from, hasta: through });
  if (owner?.value) search.set("usuario", owner.value);
  return search;
}

function remember(from, through) {
  const search = rangeParams(from, through);
  history.replaceState(null, '', location.pathname + '?' + search);
}

function element(tag, contents, classes) {
  const node = document.createElement(tag);
  if (contents !== undefined) node.textContent = contents;
  if (classes) node.className = classes;
  return node;
}

function offlineCharts(tasks, from, through) {
  const counts = new Map();
  for (const task of tasks) counts.set(task.activity_date, (counts.get(task.activity_date) ?? 0) + 1);
  const bars = [];
  const days = (Date.parse(through) - Date.parse(from)) / day + 1;
  for (let time = Date.parse(from); tasks.length && time <= Date.parse(through);) {
    const start = new Date(time);
    const date = iso(start);
    let next;
    if (days <= 31) next = time + day;
    else if (days <= 90) next = time + (7 - (start.getUTCDay() + 6) % 7) * day;
    else if (days <= 730) next = Date.UTC(start.getUTCFullYear(), start.getUTCMonth() + 1, 1);
    else next = Date.UTC(start.getUTCFullYear() + 1, 0, 1);
    const end = iso(new Date(Math.min(next - day, Date.parse(through))));
    const label = days <= 31 ? short(date) : days <= 90 ? `${short(date)}–${short(end)}`
      : days <= 730 ? `${date.slice(5, 7)}/${date.slice(0, 4)}` : date.slice(0, 4);
    let count = 0;
    for (const [key, value] of counts) if (key >= date && key <= end) count += value;
    bars.push({ label, count, from: date, through: end });
    time = next;
  }
  return [{ key: 'date', title: 'Evolución de tareas', bars }, ...groupFields.map(([key, title]) => {
    const groups = new Map();
    for (const task of tasks) groups.set(task[key], (groups.get(task[key]) ?? 0) + 1);
    const bars = [...groups].map(([label, count]) => ({ label, count }));
    bars.sort((a, b) => b.count - a.count || compare(a.label, b.label));
    return { key, title, bars };
  })];
}

function details(chart, bar, tasks) {
  document.querySelector('#chart-detail-title').textContent = chart.key === 'date'
    ? (bar.from === bar.through ? printable(bar.from) : printable(bar.from) + ' al ' + printable(bar.through))
    : bar.label;
  document.querySelector('#chart-detail-total').textContent = bar.count + (bar.count === 1 ? ' tarea' : ' tareas');
  detailRows.replaceChildren();
  for (const task of tasks) {
    const matches = chart.key === 'date'
      ? task.activity_date >= bar.from && task.activity_date <= bar.through
      : task[chart.key] === bar.label;
    if (!matches) continue;
    const link = element('a', task.work);
    link.href = '/tareas/' + encodeURIComponent(task.id);
    const row = element('li');
    row.append(link, element('span', [printable(task.activity_date), task.store, task.status].join(' · ')));
    detailRows.append(row);
  }
  if (!bar.count) detailRows.append(element('li', 'Sin tareas en este grupo.'));
  dialog.showModal();
}

function draw(charts, tasks) {
  const statuses = charts.find(chart => chart.key === 'status');
  const metrics = [['Total de tareas', tasks.length], ['Tiendas con actividad', new Set(tasks.map(task => task.store)).size],
    ...(statuses?.bars ?? []).map(bar => [bar.label, bar.count])];
  for (const [label, value] of metrics) {
    const item = element('div', undefined, 'chart-metric');
    item.append(element('span', label), element('strong', value));
    summary.append(item);
  }
  const order = ['date', 'status', 'store', 'work', 'intervention'];
  for (const chart of [...charts].sort((a, b) => order.indexOf(a.key) - order.indexOf(b.key))) {
    const card = element('section', undefined, 'paper-card chart-card');
    if (chart.key === 'date') card.classList.add('chart-timeline');
    card.append(element('h2', chart.title));
    if (chart.key === 'status') {
      card.classList.add('chart-status-card');
      const colors = ['#c11130', '#941128', '#cf777f', '#8b8b8b', '#c8c8c8'];
      let offset = 0;
      const stops = chart.bars.map((bar, index) => {
        const start = offset;
        offset += bar.count / tasks.length * 100;
        return `${colors[index % colors.length]} ${start}% ${offset}%`;
      });
      const donut = element('div', undefined, 'chart-donut');
      donut.style.background = `conic-gradient(${stops.join(',')})`;
      const center = element('div', undefined, 'chart-donut-center');
      center.append(element('strong', tasks.length), element('span', 'Tareas'));
      donut.append(center);
      donut.setAttribute('aria-hidden', 'true');
      card.append(donut);
    }
    const max = chart.bars.reduce((max, bar) => Math.max(max, bar.count), 1);
    const series = element('ul', undefined, 'chart-series');
    if (chart.key === 'date') series.classList.add('chart-columns');
    for (const bar of chart.bars) {
      const button = element('button', undefined, 'chart-bar');
      button.type = 'button';
      button.title = `${bar.label}: ${bar.count} tareas`;
      const meter = element('meter');
      meter.max = max;
      meter.value = bar.count;
      meter.setAttribute('aria-hidden', 'true');
      const amount = chart.key === 'status' ? `${bar.count} · ${(bar.count / tasks.length * 100).toLocaleString('es-PY', { maximumFractionDigits: 1 })}%` : bar.count;
      if (chart.key === 'date') {
        const column = element('span', undefined, 'chart-column');
        column.style.height = `${bar.count / max * 100}%`;
        button.setAttribute('aria-label', `${bar.label}: ${bar.count} tareas`);
        button.append(column, element('span', bar.label, 'chart-column-label'));
      } else {
        button.append(element('span', bar.label), element('strong', amount), meter);
      }
      button.addEventListener('click', () => details(chart, bar, tasks));
      const item = element('li');
      item.append(button);
      series.append(item);
    }
    card.append(series);
    grid.append(card);
  }
}

function clear() {
  if (dialog.open) dialog.close();
  detailRows.replaceChildren();
  grid.replaceChildren();
  summary.replaceChildren();
  pdf.hidden = true;
}

async function load(implicit = false, preset) {
  const current = ++generation;
  controller?.abort();
  controller = new AbortController();
  clearTimeout(expiryTimer);
  clear();
  status.textContent = 'Cargando actividad…';
  let from = fromInput.value;
  let through = throughInput.value;
  if (!implicit && (!from || !through || from > through)) {
    status.textContent = 'Selecciona dos fechas válidas en orden.';
    return;
  }
  try {
    if (!snapshotMode && navigator.onLine) {
      let response;
      const params = rangeParams(from, through);
      try {
        response = await fetch('/api/graficos?' + params, { signal: controller.signal });
      } catch (error) {
        if (error.name === 'AbortError') return;
      }
      if (response) {
        if (response.status === 401) {
          location.assign('/login');
          return;
        }
        if (!response.ok) throw new Error('No se pudo consultar el período.');
        const { charts, tasks, account_id } = await response.json();
        if (current !== generation) return;
        if (account_id !== account) {
          location.replace('/');
          return;
        }
        remember(from, through);
        status.textContent = tasks.length ? tasks.length + ' tareas en el período.' : 'No hay tareas en este período.';
        if (tasks.length) {
          params.set('formato', 'pdf');
          pdf.href = '/exportar/descargar?' + params;
          pdf.hidden = false;
          draw(charts, tasks);
        }
        return;
      }
    }
    if (owner) {
      owner.value = "";
      owner.disabled = true;
    }
    const snapshot = await chartSnapshot(account);
    if (current !== generation) return;
    if (!snapshot) {
      status.textContent = 'Inicia sesión con conexión para actualizar la copia de tareas.';
      return;
    }
    const coverage = 'Copia local del ' + printable(snapshot.from) + ' al ' + printable(snapshot.date)
      + '. Actualizada el ' + asuncion.format(new Date(snapshot.expires - 8 * 60 * 60 * 1000)) + '. ';
    expiryTimer = setTimeout(() => { load(); }, Math.max(0, snapshot.expires - Date.now()));
    if (preset) {
      through = snapshot.date;
      from = iso(new Date(Date.parse(through) - (preset - 1) * day));
    } else if (implicit) {
      through = snapshot.date;
      from = [through.slice(0, 8) + '01', snapshot.from].sort()[1];
    }
    if (from < snapshot.from || through > snapshot.date) {
      status.textContent = coverage + 'Selecciona un rango dentro de la copia. El PDF requiere conexión.';
      return;
    }
    fromInput.value = from;
    throughInput.value = through;
    remember(from, through);
    const tasks = snapshot.tasks.filter(task => task.activity_date >= from && task.activity_date <= through);
    tasks.sort((a, b) => compare(b.activity_date, a.activity_date) || b.id - a.id);
    status.textContent = coverage + (tasks.length ? tasks.length + ' tareas en el período.' : 'No hay tareas en este período.');
    if (tasks.length) draw(offlineCharts(tasks, from, through), tasks);
  } catch {
    if (current === generation) status.textContent = 'No se pudieron cargar los gráficos. Vuelve a aplicar el rango.';
  }
}

function applyRange() {
  if (!form.reportValidity()) return;
  if (fromInput.value > throughInput.value) {
    throughInput.setCustomValidity('La fecha final debe ser igual o posterior a la inicial.');
    throughInput.reportValidity();
    return;
  }
  load();
}

form.addEventListener('submit', event => {
  event.preventDefault();
  applyRange();
});
form.addEventListener('input', () => throughInput.setCustomValidity(''));
for (const button of form.querySelectorAll('[data-days]')) {
  button.addEventListener('click', () => {
    const days = Number(button.dataset.days);
    throughInput.setCustomValidity('');
    throughInput.value = panel.dataset.through;
    fromInput.value = iso(new Date(Date.parse(throughInput.value) - (days - 1) * day));
    load(false, days);
  });
}
window.addEventListener('offline', () => load());
window.addEventListener('online', () => {
  if (snapshotMode) {
    location.replace('/graficos' + location.search);
    return;
  }
  if (owner) owner.disabled = false;
  load();
});
document.addEventListener('visibilitychange', () => {
  if (!document.hidden && (snapshotMode || !navigator.onLine)) load();
});
window.addEventListener('pageshow', event => {
  if (event.persisted) load();
});
if (typeof BroadcastChannel === 'function') {
  const channel = new BroadcastChannel('taskapp-device');
  channel.addEventListener('message', event => {
    const switched = account && event.data?.accountId && event.data.accountId !== account;
    if (event.data === 'logout' || switched) {
      generation++;
      clearTimeout(expiryTimer);
      clear();
      location.replace(switched ? '/' : '/login');
    }
  });
}
load(!hasRange);
