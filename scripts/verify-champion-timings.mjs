import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const datasetUrl = new URL("../data/champions/champion-timings.v1.json", import.meta.url);
const visualCatalogUrl = new URL("../assets/simulation/champions.json", import.meta.url);
const datasetPath = fileURLToPath(datasetUrl);
const timingValues = new Set(["Early", "Mid", "Late", "Unknown"]);
const roleValues = new Set(["Top", "Jungle", "Mid", "Bot", "Support"]);

function fail(message) {
  throw new Error(`[champion-timings] ${message}`);
}

let dataset;
try {
  dataset = JSON.parse(readFileSync(datasetPath, "utf8"));
} catch (error) {
  fail(`required dataset is unavailable or invalid at ${datasetPath}: ${error.message}`);
}

if (!dataset || typeof dataset !== "object" || Array.isArray(dataset)) {
  fail("dataset must be a JSON object");
}

if (dataset.schemaVersion !== 2) {
  fail("schemaVersion must be 2");
}

if (dataset.dataset !== "champion-timings") {
  fail('dataset must be "champion-timings"');
}

if (dataset.defaultTiming !== "Unknown") {
  fail('defaultTiming must be "Unknown"');
}

if (!dataset.data || typeof dataset.data !== "object" || Array.isArray(dataset.data)) {
  fail("data must be an object keyed by champion id");
}

if (!dataset.catalog || typeof dataset.catalog !== "object") fail("catalog metadata must be an object");
if (!dataset.methodology || typeof dataset.methodology !== "object") fail("methodology must be an object");
if (!Array.isArray(dataset.sources) || dataset.sources.length === 0) fail("sources must be a non-empty array");

const hasDerivedNonOfficialDisclosure = (value) =>
  typeof value === "string" && /derived/i.test(value) && /(?:not|never) an official/i.test(value);

if (!hasDerivedNonOfficialDisclosure(dataset.methodology.classification)) {
  fail("methodology.classification must explicitly state that claims are derived and not official");
}

const sourceIds = new Set();
for (const source of dataset.sources) {
  if (!source || typeof source !== "object" || typeof source.id !== "string" || !source.id || typeof source.url !== "string" || !source.url) {
    fail("every source must contain a non-empty id and url");
  }
  if (sourceIds.has(source.id)) fail(`duplicate source id: ${source.id}`);
  sourceIds.add(source.id);
}

let visualCatalog;
try {
  visualCatalog = JSON.parse(readFileSync(fileURLToPath(visualCatalogUrl), "utf8"));
} catch (error) {
  fail(`visual champion catalog is unavailable or invalid: ${error.message}`);
}
const visualIds = Object.keys(visualCatalog?.data?.roles ?? {}).sort();
const visualRolesByChampion = visualCatalog?.data?.roles;
if (!visualRolesByChampion || typeof visualRolesByChampion !== "object" || Array.isArray(visualRolesByChampion)) {
  fail("visual champion catalog roles must be an object keyed by champion id");
}

for (const [championId, entry] of Object.entries(dataset.data)) {
  if (!championId) fail("data contains an empty champion id");
  if (!entry || typeof entry !== "object" || Array.isArray(entry)) fail(`data.${championId} must be an object`);
  if (entry.champion_id !== championId) fail(`data.${championId}.champion_id must match its key`);
  if (!Array.isArray(entry.roles) || entry.roles.length === 0 || entry.roles.some((role) => !roleValues.has(role))) {
    fail(`data.${championId}.roles must contain supported roles`);
  }
  if (championId in visualRolesByChampion && JSON.stringify(entry.roles) !== JSON.stringify(visualRolesByChampion[championId])) {
    fail(`data.${championId}.roles must exactly match the visual catalog`);
  }
  if (!timingValues.has(entry.timing)) {
    fail(`data.${championId}.timing must be one of: ${[...timingValues].join(", ")}`);
  }
  if (!['derived', 'unknown'].includes(entry.status)) fail(`data.${championId}.status must be derived or unknown`);
  if (!['low', 'medium'].includes(entry.confidence)) fail(`data.${championId}.confidence must be low or medium`);
  if (entry.scope !== 'role-invariant') fail(`data.${championId}.scope must be role-invariant`);
  if (!hasDerivedNonOfficialDisclosure(entry.evidence)) {
    fail(`data.${championId}.evidence must explicitly state that the timing is derived and not official`);
  }
  if (!Array.isArray(entry.source_basis) || entry.source_basis.length === 0 || entry.source_basis.some((id) => !sourceIds.has(id))) {
    fail(`data.${championId}.source_basis must reference known sources`);
  }
  if (!Array.isArray(entry.review_flags)) fail(`data.${championId}.review_flags must be an array`);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(entry.last_verified)) fail(`data.${championId}.last_verified must be ISO date`);
  if (!dataset.catalog.game_patch || !dataset.catalog.ddragon_version || !dataset.catalog.captured_at) {
    fail("catalog must record game_patch, ddragon_version, and captured_at");
  }
}

const timingIds = Object.keys(dataset.data).sort();
const missingFromDataset = visualIds.filter((id) => !timingIds.includes(id));
const extrasOverVisual = timingIds.filter((id) => !visualIds.includes(id));
if (missingFromDataset.length > 0) fail(`missing visual catalog IDs: ${missingFromDataset.join(", ")}`);
if (extrasOverVisual.join(",") !== "Heimerdinger,Quinn") {
  fail(`expected only DDragon additions Heimerdinger and Quinn; found: ${extrasOverVisual.join(", ") || "none"}`);
}

console.log(`[champion-timings] validated ${timingIds.length} entries; visual=${visualIds.length}; missing=${missingFromDataset.length}; extra=${extrasOverVisual.join(",")}`);
