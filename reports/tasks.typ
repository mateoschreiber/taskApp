#let data = json("report.json")
#let ink = rgb("#231d20")
#let red = rgb("#df122d")
#let wine = rgb("#600f24")
#let muted = rgb("#665d60")
#let rule = rgb("#ded7d4")
#let paper = rgb("#f7f5f2")
#let blush = rgb("#f4dce0")
#let mono = "DejaVu Sans Mono"
#let months = ("enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre")
#let weekdays = ("lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo")

#set document(title: "Trabajos realizados · taskApp", author: "taskApp", description: "Registro de actividad del " + data.period)
#set text(font: "Libertinus Serif", size: 11pt, lang: "es", fill: ink)
#set par(leading: 0.57em)

#let caption(value, color: muted) = text(font: mono, size: 8pt, fill: color, tracking: 0.25pt, value)
#let plain(value) = {
  // Keep long identifiers printable in narrow columns. All values remain literal text.
  text(value.replace(regex("[^\\s]{16}"), m => m.text + "\u{200b}"))
}
#let formatted-date(value) = {
  let parts = value.split("-").map(int)
  let date = datetime(year: parts.at(0), month: parts.at(1), day: parts.at(2))
  weekdays.at(date.weekday() - 1) + ", " + str(date.day()) + " de " + months.at(date.month() - 1) + " de " + str(date.year())
}
#let divider(color: rule) = line(length: 100%, stroke: 0.6pt + color)
#set page(
  paper: "a4", margin: (x: 18mm, top: 26mm, bottom: 21mm),
  header: block(width: 100%)[
    #grid(columns: (auto, 1fr, auto), column-gutter: 5pt, align: (left + horizon, left + horizon, right + horizon),
      image("brand.png", width: 17pt, alt: none),
      text(13pt, weight: "bold")[taskApp],
      caption("REGISTRO DE TRABAJO", color: wine))
    #v(3pt)
    #divider()
  ],
  footer: block(width: 100%)[
    #divider()
    #v(4pt)
    #caption(data.period) #h(1fr) #caption([Página #context counter(page).display("1 / 1", both: true)])
  ],
)

#let metric(number, label, primary: false) = block(width: 100%, inset: (x: 11pt, y: 9pt),
  fill: if primary { blush } else { paper }, radius: 4pt,
  stack(spacing: 4pt, text(23pt, weight: "bold", fill: wine, str(number)), caption(label, color: wine)))
#let detail(label, value) = stack(spacing: 3pt, caption(label), text(size: 11pt, plain(value)))

#block(width: 100%, inset: (x: 16pt, y: 13pt), radius: 5pt, fill: wine)[
  #grid(columns: (1.15fr, 1fr), gutter: 5pt, align: left + horizon,
    stack(spacing: 9pt,
      text(30pt, fill: white, tracking: -0.5pt, weight: "bold")[Trabajos realizados],
      text(12pt, fill: white, data.period)),
    image("jornada.png", width: 100%, height: 89pt, fit: "contain", alt: none))
]
#v(2pt)
#grid(columns: (1fr, 1fr, 1fr), column-gutter: 5pt,
  metric(data.tasks.len(), "TAREAS", primary: true),
  metric(data.tasks.map(t => t.store).dedup().len(), "TIENDAS"),
  metric(data.tasks.map(t => t.activity_date).dedup().len(), "DÍAS CON ACTIVIDAD"),
)

#v(6pt)
#block(sticky: true)[#caption("DISTRIBUCIÓN POR ESTADO", color: wine)]
#v(3pt)
#grid(columns: (1fr, 1fr), column-gutter: 16pt, row-gutter: 5pt,
  ..data.states.pairs().map(pair => block(width: 100%, breakable: false)[
    #grid(columns: (1fr, auto), gutter: 5pt,
      text(size: 10pt, plain(pair.at(0))), text(font: mono, size: 9pt, str(pair.at(1))))
    #v(1pt)
    #block(width: 100%, height: 2pt, fill: rule)[
      #rect(width: pair.at(1) / data.tasks.len() * 100%, height: 2pt, fill: red, stroke: none)
    ]
  ])
)

#let previous = ""
#for task in data.tasks {
  if task.activity_date != previous {
    previous = task.activity_date
    v(9pt)
    block(width: 100%, sticky: true)[
      #divider(color: wine)
      #v(4pt)
      #grid(columns: (3pt, 1fr), gutter: 6pt, align: left + horizon,
        rect(fill: red, stroke: none, width: 3pt, height: 14pt),
        text(16pt, weight: "bold", fill: wine, formatted-date(previous)))
    ]
  }
  block(width: 100%, breakable: true, above: 5pt, below: 3pt, inset: (left: 10pt, y: 3pt), stroke: (left: 1.2pt + rule))[
    #block(sticky: true)[
      #grid(columns: (1fr, auto), gutter: 8pt, align: (left + horizon, right + horizon),
        text(size: 11pt, fill: wine, weight: "bold", plain(task.store)),
        caption("REG. " + str(task.id)))
      #v(0pt)
      #text(16pt, weight: "bold", plain(task.work))
    ]
    #v(5pt)
    #grid(columns: (1fr, 1fr), row-gutter: 6pt, column-gutter: 12pt,
      detail("Supervisor general", task.supervisor), detail("Zona", task.zone),
      detail("Tipo de intervención", task.intervention), detail("Estado", task.status))
    #if task.notes != "" {
      v(6pt)
      block(width: 100%, fill: paper, inset: (x: 8pt, y: 5pt), breakable: true, radius: 3pt)[
        #block(sticky: true)[#caption("Observaciones")]
        #v(1pt)
        #set text(size: 10pt)
        #for (index, row) in task.notes.split("\n").enumerate() {
          if index > 0 { linebreak() }
          plain(row)
        }
      ]
    }
  ]
}
