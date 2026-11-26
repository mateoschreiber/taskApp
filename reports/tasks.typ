#let data = json("report.json")
#let ink = rgb("#192d37")
#let accent = rgb("#176c78")
#let muted = rgb("#526874")
#let rule = rgb("#d9e6eb")
#set document(title: "Trabajos realizados · taskApp")
#set text(font: "Libertinus Serif", size: 11pt, lang: "es", fill: ink)
#set par(leading: 0.65em)
#set page(
  paper: "a4", margin: (x: 21mm, top: 24mm, bottom: 20mm),
  header: [#text(10pt, weight: "bold", fill: accent)[taskApp] #h(1fr) #text(9pt, fill: muted)[Registro de actividad]],
  footer: align(right, text(9pt, fill: muted)[Página #context counter(page).display("1 / 1", both: true)]),
)
#let readable(date) = {
  let parts = date.split("-")
  parts.at(2) + "/" + parts.at(1) + "/" + parts.at(0)
}
#let plain(value) = {
  let span = regex("[^\\s]{30}")
  text(value.replace(span, m => m.text + "\u{200b}"))
}
#let detail(label, value) = [
  #text(8.5pt, fill: muted, weight: "bold", label)
  #linebreak()
  #plain(value)
]
#text(29pt, weight: "bold")[Trabajos realizados]
#v(3pt)
#text(12pt, fill: muted)[#data.period]
#v(12pt)
#block(width: 100%, inset: 12pt, radius: 5pt, fill: rgb("#eaf4f5"))[
  #text(16pt, fill: accent, weight: "bold")[#data.tasks.len() #if data.tasks.len() == 1 [tarea] else [tareas]]
  #for (status, count) in data.states.pairs() {
    linebreak()
    text(10pt)[#plain(status) · #count]
  }
]
#let previous = ""
#for task in data.tasks {
  if task.activity_date != previous {
    previous = task.activity_date
    heading(level: 2, text(fill: accent, readable(previous)))
  }
  block(width: 100%, inset: (x: 12pt, y: 10pt), radius: 5pt, stroke: rule, breakable: true)[
    #text(15pt, weight: "bold", plain(task.work))
    #linebreak()
    #text(10pt, fill: accent, plain(task.store))
    #v(6pt)
    #grid(columns: (1fr, 1fr), gutter: 8pt,
      detail("Supervisor General", task.supervisor), detail("Zona", task.zone),
      detail("Tipo de intervención", task.intervention), detail("Estado", task.status))
    #if task.notes != "" {
      v(7pt)
      text(8.5pt, fill: muted, weight: "bold")[Detalle / observaciones]
      linebreak()
      for (index, row) in task.notes.split("\n").enumerate() {
        if index > 0 { linebreak() }
        plain(row)
      }
    }
  ]
  v(4pt)
}
