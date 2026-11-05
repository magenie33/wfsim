// SPDX-License-Identifier: AGPL-3.0-or-later
// Node imports the same module and JSON packs that Wrangler bundles.
import { readFileSync } from 'node:fs';

export async function load(url, context, next) {
  if (!url.endsWith('/mcp/engine.js')) return next(url, context);
  let source = readFileSync(new URL(url), 'utf8');
  source = source.replace(/^import module from "([^"\n]+)";$/m, (_, p) =>
    'import {readFileSync as readPackFile} from "node:fs";\nconst module=new WebAssembly.Module(readPackFile(new URL('
    + JSON.stringify(new URL(p, url).href) + ')));');
  source = source.replace(/^import (pack\d+) from "([^"\n]+)";$/gm, (_, name, p) =>
    'const ' + name + '=JSON.parse(readPackFile(new URL(' + JSON.stringify(new URL(p, url).href) + '),"utf8"));');
  return { format: 'module', source, shortCircuit: true };
}
