import { startOffline } from '/offline.js';

const fold = value => value.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
const key = value => value.trim().toLowerCase();
const add = '__new__';
const queuedChoices = new WeakMap();
const viewport = window.visualViewport;
const navigation = document.querySelector('.nav');
let positionOpenChoice;
viewport?.addEventListener('resize', () => positionOpenChoice?.());

function setupChoice(field, source) {
  const select = field.querySelector('select');
  const custom = field.querySelector('.custom');
  const customInput = custom.querySelector('input');
  const label = field.querySelector('label');
  const input = document.createElement('input');
  const list = document.createElement('ul');
  let options = [...select.options].map(option => option.value).filter(value => value && value !== add);
  let pending = queuedChoices.get(source) || [];
  let entries = [];
  let active = -1;
  let committed;

  input.type = 'text';
  input.id = select.id + '-search';
  input.autocomplete = 'off';
  input.required = true;
  input.maxLength = 160;
  input.placeholder = 'Escribe para buscar';
  input.setAttribute('role', 'combobox');
  input.setAttribute('aria-autocomplete', 'list');
  input.setAttribute('aria-expanded', 'false');
  const error = field.querySelector('.field-error');
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
    if (positionOpenChoice === positionList) positionOpenChoice = null;
    list.hidden = true;
    active = -1;
    input.setAttribute('aria-expanded', 'false');
    input.removeAttribute('aria-activedescendant');
  }

  function positionList() {
    const { top, bottom } = field.getBoundingClientRect();
    const viewportTop = viewport?.offsetTop ?? 0;
    let viewportBottom = viewportTop + (viewport?.height ?? innerHeight);
    if (navigation && getComputedStyle(navigation).position === 'fixed') {
      viewportBottom = Math.min(viewportBottom, navigation.getBoundingClientRect().top);
    }
    const below = viewportBottom - bottom;
    const above = top - viewportTop;
    const up = below < 144 && above > below;
    list.classList.toggle('suggestions-up', up);
    list.style.setProperty('--suggestion-space', Math.max(44, (up ? above : below) - 8) + 'px');
  }

  function validity() {
    input.setCustomValidity(input.value && !select.value ? 'Selecciona una sugerencia o confirma el nuevo valor.' : '');
  }

  function choose(entry, interactive = false) {
    if (interactive && !input.dispatchEvent(new CustomEvent('taskapp-choose', {
      bubbles: true, cancelable: true, detail: entry.value,
    }))) {
      if (committed) {
        choose(committed);
        input.dispatchEvent(new Event('change', { bubbles: true }));
      }
      return;
    }
    select.value = entry.isNew ? add : entry.value;
    customInput.value = entry.isNew ? entry.value : '';
    input.value = entry.value;
    committed = entry;
    clearTaskError(input);
    validity();
    close();
    if (interactive) input.dispatchEvent(new Event('change', { bubbles: true }));
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
      item.addEventListener('click', () => { input.focus(); choose(entry, true); });
      list.append(item);
    }
    list.hidden = !entries.length;
    if (entries.length) {
      positionOpenChoice = positionList;
      positionList();
    } else close();
    input.setAttribute('aria-expanded', String(entries.length > 0));
  }

  input.value = select.value === add ? customInput.value : select.value;
  committed = { value: input.value, isNew: select.value === add };
  input.addEventListener('input', () => {
    const selected = options.find(option => key(option) === key(input.value));
    if (selected && field.closest('.task-store')) {
      choose({ value: selected, isNew: false }, true);
      return;
    }
    select.value = selected || '';
    customInput.value = '';
    clearTaskError(input);
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
      choose(entries[active], true);
    }
  });

  field.addEventListener('taskapp-values', event => {
    const { suggestions, queued, selected, newValue } = event.detail;
    options = suggestions;
    pending = queued;
    queuedChoices.set(field, queued);
    select.replaceChildren(new Option('', ''), ...suggestions.map(value => new Option(value, value)), new Option('', add));
    const value = newValue || selected || '';
    const resolved = options.find(option => key(option) === key(value));
    choose({ value: resolved || value, isNew: Boolean(value && !resolved) });
  });
  field.addEventListener('taskapp-reset', () => choose({ value: '', isNew: false }));
}

function clearTaskError(input) {
  const error = input.closest('.field')?.querySelector('.batch-field-error');
  if (error) {
    error.remove();
    input.removeAttribute('aria-describedby');
  }
  input.removeAttribute('aria-invalid');
}

function showTaskError(input, message) {
  const container = input.closest('.field');
  if (!container || container.querySelector('.batch-field-error')) return;
  const error = document.createElement('p');
  error.className = 'field-error batch-field-error';
  error.id = input.id + '-batch-error';
  error.textContent = message;
  container.append(error);
  input.setAttribute('aria-invalid', 'true');
  input.setAttribute('aria-describedby', error.id);
}

function setupTaskBatch(form) {
  const content = form.querySelector('[data-task-content]');
  const blocks = form.querySelector('[data-task-blocks]');
  const first = blocks.firstElementChild;
  const blueprint = first.cloneNode(true);
  const addTask = form.querySelector('[data-task-add]');
  const store = form.elements.store;
  const save = form.querySelector('[type=submit]');
  const feedback = form.querySelector('[data-task-feedback]');
  const sharedFields = ['date', 'store', 'store_new', 'supervisor', 'supervisor_new', 'zone', 'zone_new'];
  const limit = Number(form.dataset.taskBatch);
  let nextId = 0;
  let baseline;
  let committedStore;
  let enabled = true;
  let busy = false;
  let watching = false;
  let retryOnly = false;
  let submit;

  const rows = () => [...blocks.children];
  const storeName = () => store.value === add ? form.elements.store_new.value : store.value;
  const read = () => ({
    shared: Object.fromEntries(sharedFields.map(name => [name, form.elements[name].value])),
    tasks: rows().map(row => Object.fromEntries([...row.querySelectorAll('[name]')].map(field => [field.name, field.value]))),
  });
  function hasWork(row) {
    return [...row.querySelectorAll('input:not([hidden]), textarea, select')].some(field => field.value.trim());
  }
  function dirty() {
    return enabled && !form.hidden && baseline && (blocks.childElementCount !== 1
      || hasWork(blocks.firstElementChild) || sharedFields.filter(name => name !== 'store' && name !== 'store_new')
        .some(name => form.elements[name].value !== baseline[name]));
  }
  function beforeUnload(event) {
    if (dirty() || busy) {
      event.preventDefault();
      event.returnValue = true;
    }
  }
  function watch() {
    const needed = Boolean(dirty() || busy);
    if (needed === watching) return;
    watching = needed;
    if (needed) window.addEventListener('beforeunload', beforeUnload);
    else window.removeEventListener('beforeunload', beforeUnload);
  }
  function say(message = '') {
    feedback.textContent = message;
    feedback.hidden = !message;
    if (message) feedback.focus();
  }
  function reveal() {
    content.hidden = enabled && !storeName();
    content.querySelectorAll('fieldset').forEach(group => { group.disabled = content.hidden; });
    save.hidden = content.hidden;
  }
  function update() {
    reveal();
    const count = blocks.childElementCount;
    rows().forEach((row, index) => {
      row.querySelector('[data-task-title]').textContent = enabled ? 'Tarea ' + (index + 1) : 'Detalle del trabajo';
      const remove = row.querySelector('.task-remove');
      remove.hidden = !enabled || count === 1;
      remove.setAttribute('aria-label', 'Eliminar tarea ' + (index + 1));
    });
    addTask.hidden = !enabled;
    addTask.disabled = count >= limit;
    form.querySelector('[data-submit-label]').textContent = enabled
      ? (count === 1 ? 'Guardar tarea' : 'Guardar ' + count + ' tareas') : 'Guardar cambios';
    watch();
  }
  function prepare(row) {
    row.tabIndex = -1;
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'quiet task-remove';
    button.textContent = 'Eliminar';
    row.querySelector('legend').append(button);
    button.addEventListener('click', () => {
      if (blocks.childElementCount === 1 || busy) return;
      if (hasWork(row) && !window.confirm('¿Eliminar esta tarea y los datos que ingresaste?')) return;
      const next = row.nextElementSibling || row.previousElementSibling;
      row.remove();
      update();
      next.focus();
    });
  }
  function append() {
    const row = blueprint.cloneNode(true);
    const suffix = '-task-' + ++nextId;
    const originals = first.querySelectorAll('select');
    row.querySelectorAll('select').forEach((select, index) => {
      select.replaceChildren(...[...originals[index].options].map(option => new Option(option.text, option.value)));
    });
    for (const field of row.querySelectorAll('[id]')) field.id += suffix;
    for (const label of row.querySelectorAll('label[for]')) label.htmlFor += suffix;
    blocks.append(row);
    row.querySelectorAll('[data-choice]').forEach((field, index) => setupChoice(field, originals[index].closest('[data-choice]')));
    prepare(row);
    return row;
  }
  function reset(count = 1, active = true) {
    enabled = active;
    retryOnly = false;
    blocks.replaceChildren(first);
    first.querySelector('textarea').value = '';
    clearTaskError(first.querySelector('textarea'));
    first.querySelectorAll('[data-choice]').forEach(field => field.dispatchEvent(new Event('taskapp-reset')));
    for (let i = 1; i < count; i++) append();
    say();
    refresh();
  }
  function refresh() {
    committedStore = storeName();
    baseline = read().shared;
    update();
  }
  function fieldErrors(errors) {
    for (const [index, fields] of errors.entries()) {
      for (const [name, message] of Object.entries(fields)) {
        const select = sharedFields.includes(name) ? form.elements[name] : rows()[index].querySelector('[name="' + name + '"]');
        const input = select?.closest('[data-choice]')?.querySelector('[role=combobox]') || select;
        if (input) showTaskError(input, message);
      }
    }
  }
  prepare(first);
  form.addEventListener('taskapp-choose', event => {
    if (!enabled || !event.target.closest('.task-store') || event.detail === committedStore) return;
    if (dirty() && !window.confirm('¿Cambiar de sucursal? Se descartarán los detalles de trabajo ingresados.')) {
      event.preventDefault();
      return;
    }
    reset();
    committedStore = event.detail;
  });
  form.addEventListener('change', event => {
    if (event.target.closest('.task-store')) {
      committedStore = storeName() || committedStore;
      update();
    }
    watch();
  });
  form.addEventListener('input', event => {
    if (event.target.matches('input, textarea')) clearTaskError(event.target);
    if (event.target.closest('.task-store')) reveal();
    watch();
  });
  form.addEventListener('invalid', event => {
    if (enabled) showTaskError(event.target, event.target.validity.valueMissing
      ? 'Completa este campo.' : event.target.validationMessage);
  }, true);
  addTask.addEventListener('click', () => {
    if (busy || blocks.childElementCount >= limit) return;
    const row = append();
    update();
    row.focus({ preventScroll: true });
    row.scrollIntoView({ block: 'nearest', behavior: 'instant' });
  });
  form.addEventListener('submit', async event => {
    if (!enabled) return;
    event.preventDefault();
    if (busy) return;
    busy = true;
    save.disabled = true;
    const storeGroup = store.closest('fieldset');
    const groups = [storeGroup, ...content.querySelectorAll('fieldset'), addTask];
    const payload = read();
    groups.forEach(group => { group.inert = true; });
    watch();
    say();
    try {
      if (!submit) throw new Error();
      await submit(payload);
    } catch {
      say('No se pudo guardar. Tus datos siguen en el formulario. Vuelve a intentarlo.');
    } finally {
      groups.forEach(group => { group.inert = retryOnly; });
      save.disabled = false;
      busy = false;
      if (!feedback.hidden) feedback.focus();
      watch();
    }
  });
  return {
    reset, refresh, rows, say, fieldErrors,
    get enabled() { return enabled; },
    canLeave: () => !busy && !retryOnly && (!dirty() || window.confirm('¿Salir del formulario y descartar las tareas sin guardar?')),
    requireRetry: () => { retryOnly = true; },
    unlock: () => { retryOnly = false; },
    setSubmit: handler => { submit = handler; },
  };
}

const batchForm = document.querySelector('[data-task-batch]');
const batch = batchForm && setupTaskBatch(batchForm);
for (const field of document.querySelectorAll('[data-choice]')) setupChoice(field);
batch?.refresh();

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
startOffline(showConnection, batch);
