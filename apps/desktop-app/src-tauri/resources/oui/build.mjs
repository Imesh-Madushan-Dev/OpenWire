// Builds `oui.tsv` from the IEEE's public MAC address registries.
//
//   node build.mjs <oui.csv> <mam.csv> <oui36.csv>
//
// Inputs, from https://standards-oui.ieee.org/ :
//   oui/oui.csv      MA-L, 24-bit prefixes
//   oui28/mam.csv    MA-M, 28-bit prefixes
//   oui36/oui36.csv  MA-S, 36-bit prefixes
//
// Output: one `PREFIX<TAB>Vendor` line per assignment, the prefix in upper-case
// hex - 6, 7 or 9 digits, its length saying which registry it came from. Names
// are trimmed of legal suffixes and shouting so they read like brands
// ("Apple", "Huawei Technologies") in a narrow column.

import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const inputs = process.argv.slice(2)
if (inputs.length !== 3) {
  console.error('usage: node build.mjs <oui.csv> <mam.csv> <oui36.csv>')
  process.exit(1)
}

/** RFC 4180 rows: quoted fields may hold commas and doubled quotes. */
function parseCsv(text) {
  const rows = []
  let row = []
  let field = ''
  let quoted = false
  for (let i = 0; i < text.length; i++) {
    const c = text[i]
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') {
        field += '"'
        i++
      } else if (c === '"') quoted = false
      else field += c
    } else if (c === '"') quoted = true
    else if (c === ',') {
      row.push(field)
      field = ''
    } else if (c === '\n' || c === '\r') {
      if (c === '\r' && text[i + 1] === '\n') i++
      row.push(field)
      rows.push(row)
      row = []
      field = ''
    } else field += c
  }
  if (field || row.length) rows.push([...row, field])
  return rows
}

const SUFFIX =
  /[\s,]+(inc|incorporated|corp|corporation|co|company|ltd|limited|llc|l\.l\.c|gmbh|ag|s\.?a|s\.?r\.?l|b\.?v|pty|plc|oy|ab|a\/s|k\.?k|kg|sas|s\.?p\.?a|pte|co\.?,?\s*ltd|company limited|& co|ug)\.?$/i

function clean(raw) {
  // NFKC folds full-width punctuation ("CO.，LTD") into the ASCII the
  // suffix pattern expects.
  let name = raw.normalize('NFKC').replace(/\s+/g, ' ').trim()
  // Suffixes stack ("Co., Ltd.", "GmbH & Co. KG"), so strip until none is left.
  for (let previous = ''; previous !== name; ) {
    previous = name
    name = name.replace(SUFFIX, '').replace(/[\s,.]+$/, '')
  }
  // All-caps names read as shouting; title-case the plain words, but keep
  // short or punctuated ones that are acronyms or marks (HP, ZTE, TP-LINK).
  if (!/[a-z]/.test(name)) {
    name = name
      .replace(/\b[A-Z]{4,}\b/g, (word) => word[0] + word.slice(1).toLowerCase())
      .replace(/(?<=\S )(AND|OF|FOR|THE)\b/g, (word) => word.toLowerCase())
  }
  return name
}

const SKIP = /^(ieee registration authority|private)$/i

const vendors = new Map()
for (const path of inputs) {
  const [header, ...rows] = parseCsv(readFileSync(path, 'utf8'))
  const prefixAt = header.indexOf('Assignment')
  const nameAt = header.indexOf('Organization Name')
  for (const row of rows) {
    const prefix = row[prefixAt]?.trim().toUpperCase()
    const name = clean(row[nameAt] ?? '')
    if (!prefix || !/^[0-9A-F]{6,9}$/.test(prefix) || !name || SKIP.test(name)) continue
    vendors.set(prefix, name)
  }
}

const lines = [...vendors].sort(([a], [b]) => a.localeCompare(b)).map(([p, n]) => `${p}\t${n}`)
const out = join(dirname(fileURLToPath(import.meta.url)), 'oui.tsv')
writeFileSync(out, lines.join('\n') + '\n')
console.log(`wrote ${lines.length} prefixes to ${out}`)
