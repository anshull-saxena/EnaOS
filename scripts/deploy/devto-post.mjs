import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import https from "node:https";

function parseFrontMatter(markdown) {
  if (!markdown.startsWith("---\n")) return { frontMatter: {}, body: markdown };
  const endIdx = markdown.indexOf("\n---\n", 4);
  if (endIdx === -1) return { frontMatter: {}, body: markdown };
  const fmRaw = markdown.slice(4, endIdx).trimEnd();
  const body = markdown.slice(endIdx + "\n---\n".length);

  const frontMatter = {};
  for (const line of fmRaw.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const sep = trimmed.indexOf(":");
    if (sep === -1) continue;
    const key = trimmed.slice(0, sep).trim();
    let value = trimmed.slice(sep + 1).trim();
    if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
      value = value.slice(1, -1);
    }
    if (value === "true") value = true;
    if (value === "false") value = false;
    if (typeof value === "string" && value.startsWith("[") && value.endsWith("]")) {
      const inner = value.slice(1, -1).trim();
      value = inner
        ? inner
            .split(",")
            .map((s) => s.trim())
            .map((s) => s.replace(/^['"]|['"]$/g, ""))
        : [];
    }
    frontMatter[key] = value;
  }

  return { frontMatter, body };
}

function httpRequestJson(url, { method, headers, body }) {
  return new Promise((resolve, reject) => {
    const req = https.request(url, { method, headers }, (res) => {
      let data = "";
      res.setEncoding("utf8");
      res.on("data", (chunk) => (data += chunk));
      res.on("end", () => {
        const contentType = res.headers["content-type"] || "";
        const ok = res.statusCode && res.statusCode >= 200 && res.statusCode < 300;
        if (!ok) {
          return reject(
            new Error(
              `Request failed: ${res.statusCode} ${res.statusMessage}\n${contentType.includes("application/json") ? data : data.slice(0, 1000)}`,
            ),
          );
        }
        if (contentType.includes("application/json")) {
          try {
            resolve(JSON.parse(data));
          } catch (e) {
            reject(new Error(`Invalid JSON response: ${String(e)}\n${data.slice(0, 1000)}`));
          }
        } else {
          resolve(data);
        }
      });
    });
    req.on("error", reject);
    if (body) req.write(body);
    req.end();
  });
}

async function fetchJson(url, opts) {
  if (typeof fetch === "function") {
    const res = await fetch(url, opts);
    const text = await res.text();
    if (!res.ok) throw new Error(`Request failed: ${res.status} ${res.statusText}\n${text.slice(0, 1000)}`);
    try {
      return JSON.parse(text);
    } catch {
      return text;
    }
  }
  return httpRequestJson(url, opts);
}

async function main() {
  const apiKey = process.env.DEVTO_API_KEY;
  if (!apiKey) {
    console.error("Missing DEVTO_API_KEY env var.");
    process.exitCode = 2;
    return;
  }

  const mdPath = process.argv[2];
  if (!mdPath) {
    console.error("Usage: DEVTO_API_KEY=... node scripts/deploy/devto-post.mjs <path-to-post.md>");
    process.exitCode = 2;
    return;
  }

  const absPath = path.resolve(process.cwd(), mdPath);
  const raw = await fs.readFile(absPath, "utf8");
  const { frontMatter, body } = parseFrontMatter(raw);

  const article = {
    title: frontMatter.title,
    description: frontMatter.description,
    published: Boolean(frontMatter.published),
    canonical_url: frontMatter.canonical_url,
    cover_image: frontMatter.cover_image,
    tags: Array.isArray(frontMatter.tags) ? frontMatter.tags.join(", ") : frontMatter.tags,
    body_markdown: body.trimStart(),
  };

  if (!article.title || !article.body_markdown) {
    console.error("Front matter must include `title`, and markdown must include a body.");
    process.exitCode = 2;
    return;
  }

  const payload = JSON.stringify({ article });
  const result = await fetchJson("https://dev.to/api/articles", {
    method: "POST",
    headers: {
      "api-key": apiKey,
      "content-type": "application/json",
      accept: "application/json",
    },
    body: payload,
  });

  const url = result?.url || result?.path ? `https://dev.to${result.path}` : undefined;
  console.log(
    JSON.stringify(
      {
        id: result?.id,
        url: result?.url || url,
        published: result?.published,
        title: result?.title,
      },
      null,
      2,
    ),
  );
}

main().catch((err) => {
  console.error(String(err?.stack || err));
  process.exitCode = 1;
});

