import { startOffline } from '/offline.js';

const fold = value => value.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
const key = value => value.trim().toLowerCase();
const add = '__new__';

for (const field of document.querySelectorAll('[data-choice]')) {
  const select = field.querySelector('select');
  const custom = field.querySelector('.custom');
  const customInput = custom.querySelector('input');
  const label = field.querySelector('label');
  const input = document.createElement('input');
  const list = document.createElement('ul');
  let options = [...select.options].map(option => option.value).filter(value => value && value !== add);
  let pending = [];
  let entries = [];
  let active = -1;

  input.type = 'text';
  input.id = select.id + '-search';
  input.autocomplete = 'off';
  input.required = true;
  input.maxLength = 160;
  input.placeholder = 'Escribe para buscar';
  input.setAttribute('role', 'combobox');
  input.setAttribute('aria-autocomplete', 'list');
  input.setAttribute('aria-expanded', 'false');
  const error = document.getElementById(select.id + '-error');
  if (error) {
    input.setAttribute('aria-invalid', 'true');
    input.setAttribute('aria-describedby', error.id);
  }
  label.htmlFor = input.id;
  list.id = select.id + '-suggestions';
  list.className = 'suggestions';
  list.setAttribute('role', 'listbox');
  list.setAttribute('aria-label', label.textContent);
  list.hidden = true;
  input.setAttribute('aria-controls', list.id);
  select.required = false;
  select.hidden = true;
  custom.hidden = true;
  field.insertBefore(input, select);
  input.after(list);

  function close() {
    list.hidden = true;
    active = -1;
    input.setAttribute('aria-expanded', 'false');
    input.removeAttribute('aria-activedescendant');
  }

  function validity() {
    input.setCustomValidity(input.value && !select.value ? 'Selecciona una sugerencia o confirma el nuevo valor.' : '');
  }

  function choose(entry) {
    select.value = entry.isNew ? add : entry.value;
    customInput.value = entry.isNew ? entry.value : '';
    input.value = entry.value;
    validity();
    close();
  }

  function show() {
    const query = input.value.trim();
    const matches = value => fold(value).includes(fold(query));
    entries = options.filter(matches).map(value => ({ value, isNew: false }));
    for (const value of pending.filter(matches)) {
      if (!entries.some(entry => key(entry.value) === key(value))) entries.push({ value, isNew: true });
    }
    if (query && ![...options, ...pending].some(value => key(value) === key(query))) {
      entries.push({ value: query, isNew: true });
    }
    list.replaceChildren();
    active = -1;
    input.removeAttribute('aria-activedescendant');
    for (const [index, entry] of entries.entries()) {
      const item = document.createElement('li');
      item.id = list.id + '-' + index;
      item.setAttribute('role', 'option');
      item.setAttribute('aria-selected', 'false');
      item.textContent = entry.isNew ? 'Agregar «' + entry.value + '»' : entry.value;
      item.addEventListener('mousedown', event => event.preventDefault());
      item.addEventListener('click', () => { choose(entry); input.focus(); close(); });
      list.append(item);
    }
    list.hidden = !entries.length;
    input.setAttribute('aria-expanded', String(entries.length > 0));
  }

  input.value = select.value === add ? customInput.value : select.value;
  input.addEventListener('input', () => {
    select.value = options.find(option => key(option) === key(input.value)) || '';
    customInput.value = '';
    input.removeAttribute('aria-invalid');
    validity();
    show();
  });
  input.addEventListener('focus', show);
  field.addEventListener('focusout', event => {
    if (!field.contains(event.relatedTarget)) close();
  });
  input.addEventListener('keydown', event => {
    if (event.isComposing) return;
    if (event.key === 'Escape') close();
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (list.hidden) show();
      if (!entries.length) return;
      active = event.key === 'ArrowDown'
        ? (active + 1) % entries.length
        : (active < 1 ? entries.length - 1 : active - 1);
      for (const [index, item] of [...list.children].entries()) {
        item.setAttribute('aria-selected', String(index === active));
      }
      const item = list.children[active];
      input.setAttribute('aria-activedescendant', item.id);
      item.scrollIntoView({ block: 'nearest' });
    }
    if (event.key === 'Enter' && !list.hidden && active >= 0) {
      event.preventDefault();
      choose(entries[active]);
    }
  });

  field.addEventListener('taskapp-values', event => {
    const { suggestions, queued, selected, newValue } = event.detail;
    options = suggestions;
    pending = queued;
    select.replaceChildren(new Option('', ''), ...suggestions.map(value => new Option(value, value)), new Option('', add));
    const value = newValue || selected || '';
    const resolved = options.find(option => key(option) === key(value));
    choose({ value: resolved || value, isNew: Boolean(value && !resolved) });
  });
}

const stores = document.querySelector('.store-picker');
if (stores) {
  const desktop = matchMedia('(min-width: 72rem)');
  stores.open = desktop.matches;
  desktop.addEventListener('change', event => { stores.open = event.matches; });
}

const dialog = document.querySelector('#export-dialog');
document.querySelector('[data-open-export]')?.addEventListener('click', event => {
  if (typeof dialog?.showModal === 'function' && !event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey) {
    event.preventDefault();
    dialog.showModal();
  }
});
for (const form of document.querySelectorAll('.export-form')) {
  const from = form.elements.desde;
  const through = form.elements.hasta;
  function validateRange() { through.min = from.value; }
  from.addEventListener('input', validateRange);
  validateRange();
}

const connection = document.querySelector('[data-connection]');
function showConnection(online) {
  if (!connection) return;
  connection.querySelector('[data-connection-label]').textContent = online ? 'En línea' : 'Sin conexión';
  connection.classList.toggle('is-offline', !online);
  connection.hidden = false;
}
if (connection) {
  showConnection(navigator.onLine);
  window.addEventListener('online', () => showConnection(true));
  window.addEventListener('offline', () => showConnection(false));
}
if ('serviceWorker' in navigator) navigator.serviceWorker.register('/sw.js').catch(() => {});
startOffline(showConnection);
