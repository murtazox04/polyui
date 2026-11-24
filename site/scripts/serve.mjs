import { existsSync, readFileSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { dirname, extname, join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
export const siteRoot = join(here, "..", "dist");
export const prefix = "/polyui/";

const types = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json",
  ".map": "application/json",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".gif": "image/gif",
  ".woff2": "font/woff2",
  ".txt": "text/plain; charset=utf-8",
};

function resolveFile(pathname) {
  if (!pathname.startsWith(prefix)) return null;
  const inside = normalize(join(siteRoot, decodeURIComponent(pathname.slice(prefix.length))));
  if (inside !== siteRoot && !inside.startsWith(siteRoot + sep)) return null;
  if (existsSync(inside) && statSync(inside).isDirectory()) {
    const index = join(inside, "index.html");
    return existsSync(index) ? index : null;
  }
  return existsSync(inside) ? inside : null;
}

export function startServer(port) {
  const server = createServer((request, response) => {
    const pathname = new URL(request.url ?? "/", "http://localhost").pathname;
    if (pathname === prefix.slice(0, -1)) {
      response.writeHead(301, { location: prefix }).end();
      return;
    }
    const file = resolveFile(pathname);
    if (file === null) {
      const missing = join(siteRoot, "404.html");
      response.writeHead(404, { "content-type": types[".html"] }).end(existsSync(missing) ? readFileSync(missing) : "not found");
      return;
    }
    response.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream", "cache-control": "no-store" });
    response.end(readFileSync(file));
  });
  return new Promise((ready) => server.listen(port, "127.0.0.1", () => ready(server)));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const port = Number(process.env.PORT ?? 4300);
  await startServer(port);
  console.log(`site: http://127.0.0.1:${port}${prefix}`);
}
