import fs from "node:fs";
const src = fs.readFileSync("site/i18n.js", "utf8");
const inicio = src.indexOf("const EN"); const abre = src.indexOf("{", inicio);
let nivel = 0, fim = -1;
for (let i = abre; i < src.length; i++) { if (src[i] === "{") nivel++; else if (src[i] === "}") { nivel--; if (nivel === 0) { fim = i; break; } } }
const d = eval("(" + src.slice(abre, fim + 1) + ")");
fs.writeFileSync("site/i18n/en.json", JSON.stringify(d, null, 2) + "\n", "utf8");
console.log("chaves:", Object.keys(d).length);