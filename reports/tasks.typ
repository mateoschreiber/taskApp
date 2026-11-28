#let data = json("report.json")
#let ink = rgb("#262626")
#let muted = rgb("#595959")
#let red = rgb("#E30613")
#let dark-red = rgb("#B50915")
#let border = rgb("#D9D9D9")
#let pale = rgb("#F5F5F5")
#set document(title: "Informe de actividades")
#set text(font: ("Arial", "Liberation Sans"), size: 9pt, fill: ink, lang: "es")
#set par(leading: 0.45em)
#set page(paper: "a4", margin: (x: 14mm, top: 14mm, bottom: 16mm),
  footer: [#line(length: 100%, stroke: 0.7pt + red)\
    #text(size: 8pt, fill: muted)[Generado: #data.generated] #h(1fr) #text(size: 8pt, fill: muted)[Página #context counter(page).display()]]
)
#let plain(value) = text(value.replace(regex("[^\\s]{20}"), m => m.text + "\u{200b}"))
#let fmt-date(value) = {
  let p = value.split("-")
  if p.len() == 3 { p.at(2) + "/" + p.at(1) + "/" + p.at(0) } else { value }
}
#let section(title) = block(width: 100%, fill: red, inset: (x: 5pt, y: 3pt), sticky: true)[#text(fill: white, size: 10pt, weight: "bold", title)]
#text(size: 16pt, weight: "bold")[INFORME DE ACTIVIDADES]
#v(3pt)
#text(size: 9pt, fill: muted)[Período: #data.period · Responsable o equipo: #data.responsible · Generado: #data.generated]
#line(length: 100%, stroke: 1.3pt + red)
#v(5pt)
#text(size: 12pt, weight: "bold")[Resumen ejecutivo]
#v(3pt)
#let statuses = (:)
#for task in data.tasks { statuses.insert(task.status, statuses.at(task.status, default: 0) + 1) }
#table(columns: (1fr, 1fr, 1fr), stroke: 0.5pt + border, inset: 5pt, fill: pale,
  align: center,
  [#text(size: 15pt, weight: "bold", str(data.tasks.len()))\ Total de actividades],
  [#text(size: 15pt, weight: "bold", str(data.tasks.map(t => t.store).dedup().len()))\ Tiendas],
  [#text(size: 15pt, weight: "bold", str(data.tasks.map(t => t.activity_date).dedup().len()))\ Días con actividad],
)
#v(9pt)
#if data.tasks.len() > 0 {
  section("Distribución por estado")
  v(3pt)
  let max-count = calc.max(..statuses.values())
  for (label, count) in statuses.pairs() {
    grid(columns: (1fr, 2fr, auto), gutter: 6pt, align: left + horizon,
      plain(label), rect(width: count / max-count * 100%, height: 6pt, fill: dark-red, stroke: none), str(count))
    v(3pt)
  }
} else {
  text(fill: muted)[No hay actividades para el período seleccionado.]
}
#v(7pt)
#section("Registro de actividades")
#v(3pt)
#if data.tasks.len() > 0 {
  table(
    columns: (0.7fr, 1.1fr, 1.6fr, 1fr, 1.1fr),
    stroke: 0.45pt + border, inset: 4pt,
    fill: (x, y) => if y == 0 { red } else if calc.odd(y) { white } else { pale },
    table.header(..("Fecha", "Tienda", "Actividad", "Supervisor", "Estado").map(x => text(fill: white, weight: "bold", x))),
    ..data.tasks.map(t => (fmt-date(t.activity_date), t.store, t.work, t.supervisor, t.status)).flatten().map(plain),
  )
  v(8pt)
  section("Detalle y observaciones")
  for (index, t) in data.tasks.enumerate() {
    block(width: 100%, breakable: true, above: 5pt, below: 3pt)[
      #text(weight: "bold")[#(index + 1). #plain(t.work) — #plain(t.store)] \
      Zona: #plain(t.zone) · Tipo de intervención: #plain(t.intervention)
      #if t.notes != "" { [\ Observaciones: #plain(t.notes)] }
    ]
  }
}
