import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { execFileSync, spawn } from "node:child_process";
import readline from "node:readline";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const frontendDir = path.resolve(__dirname, "..");
const repoRoot = path.resolve(frontendDir, "..");

function bootHeadlessElm() {
  const bundlePath = path.join(frontendDir, "build", "headless.js");
  const code = fs.readFileSync(bundlePath, "utf8");
  const sandbox = {
    console,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
  };
  sandbox.globalThis = sandbox;
  sandbox.window = sandbox;
  sandbox.self = sandbox;
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox);

  const sentIntents = [];
  const snapshots = [];
  const normalize = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));
  const app = sandbox.Elm.HeadlessHarness.init({ flags: null });
  app.ports.sendIntent.subscribe((intent) => sentIntents.push(normalize(intent)));
  app.ports.emitHarnessSnapshot.subscribe((snap) => snapshots.push(normalize(snap)));

  const flush = () => new Promise((resolve) => setTimeout(resolve, 10));

  return { app, sentIntents, snapshots, flush };
}

function loadRealFixtureEnvelope(filename) {
  const fixturePath = path.join(frontendDir, "tests", "fixtures", filename);
  const session = JSON.parse(fs.readFileSync(fixturePath, "utf8"));
  return {
    ok: true,
    session,
    rejection: null,
  };
}

test("HeadlessHarness loads TESTDRUID fixture and emits Cycle 1 marquee/merc/temp intents", async () => {
  const { app, sentIntents, snapshots, flush } = bootHeadlessElm();
  const druidEnv = loadRealFixtureEnvelope("testdruid_anya.session.json");
  await flush();

  app.ports.receiveSessionEnvelope.send(druidEnv);
  await flush();
  const loadedSnap = snapshots.at(-1);
  assert.equal(loadedSnap.hasSession, true);
  assert.equal(loadedSnap.charName, "TESTDRUID");
  assert.equal(loadedSnap.dirty, false);
  assert.equal(loadedSnap.decodeError, null);
  assert.equal(loadedSnap.activeView, "ViewB");
  assert.equal(loadedSnap.mercLevel, 28);
  assert.equal(loadedSnap.mercAct, 2);
  assert.equal(loadedSnap.mercPersistenceSupported, false);
  assert.equal(loadedSnap.exportSupported, true);

  app.ports.receiveHarnessCommand.send({ kind: "SwitchView", view: "ViewA" });
  await flush();
  assert.equal(snapshots.at(-1).activeView, "ViewA");
  app.ports.receiveHarnessCommand.send({ kind: "SelectContainerFilter", filter: "stash" });
  await flush();
  assert.equal(snapshots.at(-1).containerFilter, "stash");
  assert.equal(sentIntents.length, 0);

  app.ports.receiveHarnessCommand.send({ kind: "SelectMercAct", actIndex: 5 });
  await flush();
  assert.deepEqual(sentIntents.at(-1), { SelectMercAct: { act_index: 5 } });

  app.ports.receiveHarnessCommand.send({ kind: "AdjustMercLevel", delta: 5 });
  await flush();
  assert.deepEqual(sentIntents.at(-1), { SetMercLevel: { level: 33 } });

  // Marquee Activate (green) -> re-click cancels -> Deactivate (red) drag selection -> commit batch
  app.ports.receiveHarnessCommand.send({ kind: "TogglePaintMode", mode: "Activate" });
  await flush();
  assert.equal(snapshots.at(-1).paintMode, "Activate");

  app.ports.receiveHarnessCommand.send({ kind: "TogglePaintMode", mode: "Activate" });
  await flush();
  assert.equal(snapshots.at(-1).paintMode, null);

  app.ports.receiveHarnessCommand.send({ kind: "TogglePaintMode", mode: "Deactivate" });
  await flush();
  app.ports.receiveHarnessCommand.send({
    kind: "BeginMarquee",
    dragKind: "DragQuests",
    x: 12,
    y: 16,
    ids: ["a1q1"],
  });
  await flush();
  app.ports.receiveHarnessCommand.send({
    kind: "MoveMarquee",
    x: 140,
    y: 90,
    ids: ["a1q1", "a1q2"],
  });
  await flush();
  assert.deepEqual(snapshots.at(-1).marqueeTargets, ["a1q1", "a1q2"]);

  app.ports.receiveHarnessCommand.send({
    kind: "ExtendMarquee",
    dragKind: "DragQuests",
    id: "a1q3",
  });
  await flush();
  assert.deepEqual(snapshots.at(-1).marqueeTargets, ["a1q1", "a1q2", "a1q3"]);

  app.ports.receiveHarnessCommand.send({ kind: "CommitMarquee", difficulty: "Normal" });
  await flush();
  assert.equal(snapshots.at(-1).marqueeTargets, null);
  assert.deepEqual(sentIntents.at(-1), {
    SetQuestsBatch: {
      difficulty: "Normal",
      keys: ["a1q1", "a1q2", "a1q3"],
      completed: false,
    },
  });

  app.ports.receiveHarnessCommand.send({ kind: "ToggleAllDiffQuests" });
  await flush();
  assert.deepEqual(sentIntents.at(-1), { SetAllDiffQuests: { completed: true } });

  app.ports.receiveHarnessCommand.send({ kind: "ClearTempInventory" });
  await flush();
  assert.equal(sentIntents.at(-1), "ClearTempInventory");

  app.ports.receiveSessionEnvelope.send({
    ok: true,
    session: { ...druidEnv.session, dirty: true },
    rejection: null,
  });
  await flush();
  assert.equal(snapshots.at(-1).dirty, true);
});

test("HeadlessHarness also decodes amazon_10_scrolls fixture cleanly", async () => {
  const { app, snapshots, flush } = bootHeadlessElm();
  const amazonEnv = loadRealFixtureEnvelope("amazon_10_scrolls.session.json");
  await flush();

  app.ports.receiveSessionEnvelope.send(amazonEnv);
  await flush();
  const snap = snapshots.at(-1);
  assert.equal(snap.hasSession, true);
  assert.equal(snap.charName, "TESTAMAZON");
  assert.equal(snap.decodeError, null);
});

function latestProjectionSrcMtimeMs() {
  const projDir = path.join(repoRoot, "src", "projection");
  let latest = 0;
  for (const name of ["mod.rs", "host.rs"]) {
    const filePath = path.join(projDir, name);
    if (fs.existsSync(filePath)) {
      latest = Math.max(latest, fs.statSync(filePath).mtimeMs);
    }
  }
  return latest;
}

function resolveFreshHostBinary() {
  const binName = process.platform === "win32" ? "ortho_host_bridge.exe" : "ortho_host_bridge";
  const releaseBin = path.join(repoRoot, "target", "release", binName);
  const debugBin = path.join(repoRoot, "target", "debug", binName);
  const candidate = fs.existsSync(releaseBin) ? releaseBin : fs.existsSync(debugBin) ? debugBin : null;
  const srcMtime = latestProjectionSrcMtimeMs();

  if (candidate && fs.statSync(candidate).mtimeMs >= srcMtime) {
    return candidate;
  }

  execFileSync("cargo", ["build", "--release", "--bin", "ortho_host_bridge"], {
    cwd: repoRoot,
    env: { ...process.env, CARGO_INCREMENTAL: "0" },
    stdio: "inherit",
  });
  return releaseBin;
}

function spawnRustHostClient() {
  const resolvedBin = resolveFreshHostBinary();
  const child = spawn(resolvedBin, [], { cwd: repoRoot, stdio: ["pipe", "pipe", "pipe"] });

  const rl = readline.createInterface({ input: child.stdout, crlfDelay: Infinity });
  const waiters = [];
  let stderrTail = "";
  child.stderr.on("data", (chunk) => {
    stderrTail = (stderrTail + chunk.toString("utf8")).slice(-2000);
  });
  rl.on("line", (line) => {
    const next = waiters.shift();
    if (next) {
      try {
        next.resolve(JSON.parse(line));
      } catch (err) {
        next.reject(new Error(`Invalid JSON from ortho_host_bridge: ${line} (${err})`));
      }
    }
  });
  child.on("exit", (code) => {
    while (waiters.length > 0) {
      const w = waiters.shift();
      w.reject(new Error(`ortho_host_bridge exited early (${code}): ${stderrTail}`));
    }
  });

  return {
    send(payload) {
      return new Promise((resolve, reject) => {
        waiters.push({ resolve, reject });
        child.stdin.write(JSON.stringify(payload) + "\n");
      });
    },
    async close() {
      rl.close();
      if (!child.killed) {
        child.stdin.end();
      }
      await new Promise((resolve) => {
        if (child.exitCode !== null || child.signalCode !== null) {
          resolve();
          return;
        }
        const timer = setTimeout(() => {
          child.kill();
          resolve();
        }, 1500);
        child.once("exit", () => {
          clearTimeout(timer);
          resolve();
        });
      });
    },
  };
}

function createClosedLoopDriver() {
  const elm = bootHeadlessElm();
  const host = spawnRustHostClient();
  let forwardedIntentCount = 0;

  async function applyHostEnvelope(resp) {
    elm.app.ports.receiveSessionEnvelope.send(resp.envelope);
    await elm.flush();
    return elm.snapshots.at(-1);
  }

  return {
    async requestHost(payload) {
      const resp = await host.send(payload);
      const snap = await applyHostEnvelope(resp);
      return { resp, snap };
    },
    async stepCommand(cmdObj) {
      elm.app.ports.receiveHarnessCommand.send(cmdObj);
      await elm.flush();
      const pendingIntents = elm.sentIntents.slice(forwardedIntentCount);
      forwardedIntentCount = elm.sentIntents.length;

      let lastResp = null;
      for (const intent of pendingIntents) {
        lastResp = await host.send({ cmd: "dispatch_intent", intent });
        elm.app.ports.receiveSessionEnvelope.send(lastResp.envelope);
        await elm.flush();
      }
      return {
        emittedIntents: pendingIntents,
        resp: lastResp,
        snap: elm.snapshots.at(-1),
      };
    },
    async exportAndReload(label) {
      const exported = await host.send({ cmd: "export_save_hex" });
      assert.equal(exported.ok, true);
      assert.ok(exported.export_hex && exported.export_byte_len > 100);
      const reloaded = await host.send({
        cmd: "load_save_hex",
        label,
        hex: exported.export_hex,
      });
      assert.equal(reloaded.ok, true);
      const snap = await applyHostEnvelope(reloaded);
      return { exported, reloaded, snap };
    },
    async close() {
      await host.close();
    },
  };
}

test("Closed-loop scenario 1: full multi-step browser journey across Elm HeadlessHarness and live Rust EditorSession", async () => {
  const driver = createClosedLoopDriver();
  try {
    // 1. Load real TESTDRUID save fixture into Rust host -> project to Elm
    const loadRes = await driver.requestHost({
      cmd: "load_fixture_path",
      path: "tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s",
    });
    assert.equal(loadRes.resp.ok, true);
    assert.equal(loadRes.snap.charName, "TESTDRUID");
    assert.equal(loadRes.snap.dirty, false);
    assert.equal(loadRes.snap.resDifficulty, "Normal");
    const normalFireRes = loadRes.snap.fireRes;
    const initialHellQuests = loadRes.snap.hellQuestCount;

    // 2. Seed one temporary item in host dock, switch to View A, filter stash, select item 0
    const seeded = await driver.requestHost({
      cmd: "seed_temp_item",
      card: {
        index: 99,
        code: "r01",
        name_en: "El Rune",
        name_ko: "El Rune",
        quality: "QualityNormal",
        container: "TempInventory",
        slot_label: null,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        item_level: 1,
        base_attributes: ["Defense: 15"],
        properties: ["+15 Defense"],
        set_name: null,
        set_pieces: [],
        piece_set_bonuses: [],
        set_bonuses: [],
        socketed_names: [],
        image_url: null,
      },
    });
    assert.equal(seeded.snap.tempCount, 1);

    const viewA = await driver.stepCommand({ kind: "SwitchView", view: "ViewA" });
    assert.equal(viewA.emittedIntents.length, 0);
    assert.equal(viewA.snap.activeView, "ViewA");

    const filterStash = await driver.stepCommand({ kind: "SelectContainerFilter", filter: "stash" });
    assert.equal(filterStash.emittedIntents.length, 0);
    assert.equal(filterStash.snap.containerFilter, "stash");

    const selectItem = await driver.stepCommand({ kind: "SelectItem", index: 0 });
    assert.deepEqual(selectItem.emittedIntents, [{ SelectItemForMove: { item_index: 0 } }]);
    assert.equal(selectItem.snap.selectedItemIndex, 0);

    // 3. Return to View B and verify Rust re-projects Hell resistance penalty
    await driver.stepCommand({ kind: "SwitchView", view: "ViewB" });
    const hellResStep = await driver.stepCommand({ kind: "SelectResDifficulty", difficulty: "Hell" });
    assert.deepEqual(hellResStep.emittedIntents, [{ SelectResDifficulty: { difficulty: "Hell" } }]);
    assert.equal(hellResStep.snap.resDifficulty, "Hell");
    assert.ok(
      hellResStep.snap.fireRes < normalFireRes,
      `Expected Hell fire res (${hellResStep.snap.fireRes}) < Normal (${normalFireRes})`
    );
    assert.equal(hellResStep.snap.dirty, false);

    // 4. Boundary rejection & recovery via Toast & Undo Resilience
    const rejected = await driver.requestHost({
      cmd: "dispatch_intent",
      intent: { SetMercLevel: { level: 120 } },
    });
    assert.equal(rejected.resp.ok, false);
    assert.equal(rejected.snap.rejectionCode, "merc_level_out_of_range");
    assert.match(rejected.snap.statusBanner, /merc_level_out_of_range/);
    assert.equal(rejected.snap.hasToast, true);
    assert.equal(rejected.snap.toastRawCode, "merc_level_out_of_range");
    assert.equal(rejected.snap.dirty, false);

    const dismissedToast = await driver.stepCommand({ kind: "DismissToast" });
    assert.equal(dismissedToast.snap.hasToast, false);

    const clearedBanner = await driver.stepCommand({ kind: "ClearBanner" });
    assert.equal(clearedBanner.snap.statusBanner, null);
    assert.equal(clearedBanner.snap.rejectionCode, null);

    // 5. Locked Merc Act + Step +5 are honestly rejected by Rust (`merc_persistence_not_supported`) without dirtying save
    assert.equal(loadRes.snap.mercPersistenceSupported, false);
    assert.equal(loadRes.snap.exportSupported, true);
    const actLocked = await driver.stepCommand({ kind: "SelectMercAct", actIndex: 5 });
    assert.equal(actLocked.resp.ok, false);
    assert.equal(actLocked.snap.rejectionCode, "merc_persistence_not_supported");
    assert.equal(actLocked.snap.dirty, false);
    assert.equal(actLocked.snap.mercAct, loadRes.snap.mercAct);
    await driver.stepCommand({ kind: "ClearBanner" });

    const mercStepLocked = await driver.stepCommand({ kind: "AdjustMercLevel", delta: 5 });
    assert.equal(mercStepLocked.resp.ok, false);
    assert.equal(mercStepLocked.snap.rejectionCode, "merc_persistence_not_supported");
    assert.equal(mercStepLocked.snap.dirty, false);
    assert.equal(mercStepLocked.snap.mercLevel, loadRes.snap.mercLevel);
    await driver.stepCommand({ kind: "ClearBanner" });

    // 6. Green Marquee quest activation in Hell -> dirty=true
    await driver.stepCommand({ kind: "TogglePaintMode", mode: "Activate" });
    await driver.stepCommand({
      kind: "BeginMarquee",
      dragKind: "DragQuests",
      x: 10,
      y: 12,
      ids: ["a1q1"],
    });
    await driver.stepCommand({
      kind: "MoveMarquee",
      x: 120,
      y: 80,
      ids: ["a1q1", "a5q3"],
    });
    const marqueeCommit = await driver.stepCommand({ kind: "CommitMarquee", difficulty: "Hell" });
    assert.equal(marqueeCommit.snap.dirty, true);
    assert.ok(marqueeCommit.snap.hellQuestCount >= initialHellQuests + 1);

    // 7. Clear temporary inventory in View A
    await driver.stepCommand({ kind: "SwitchView", view: "ViewA" });
    const clearedTemp = await driver.stepCommand({ kind: "ClearTempInventory" });
    assert.equal(clearedTemp.snap.tempCount, 0);

    // 8. Export binary .d2s from live Rust host and re-load to confirm persistence + checksum validity
    const { snap: reloadedSnap } = await driver.exportAndReload("scenario1-loopback.d2s");
    assert.equal(reloadedSnap.dirty, false);
    assert.equal(reloadedSnap.charName, "TESTDRUID");
    assert.ok(reloadedSnap.hellQuestCount >= initialHellQuests + 1);
  } finally {
    await driver.close();
  }
});

test("Closed-loop scenario 2: amazon_10_scrolls waypoint & difficulty toggle loopback", async () => {
  const driver = createClosedLoopDriver();
  try {
    const loadRes = await driver.requestHost({
      cmd: "load_fixture_path",
      path: "tests/fixtures/savegames/original/amazon_10_scrolls.d2s",
    });
    assert.equal(loadRes.resp.ok, true, loadRes.resp.error?.message);
    assert.equal(loadRes.snap.charName, "TESTAMAZON");
    const prog = loadRes.resp.envelope.session.progression;
    const expectedWpCount = prog.waypoint_catalog.length;
    assert.equal(prog.is_expansion, false);
    assert.equal(expectedWpCount, 30);

    const toggledAllWps = await driver.stepCommand({ kind: "ToggleAllDiffWaypoints" });
    assert.equal(toggledAllWps.snap.dirty, true);
    assert.equal(toggledAllWps.snap.hellWaypointCount, expectedWpCount);

    const { snap: reloadedSnap } = await driver.exportAndReload("scenario2-amazon-loopback.d2s");
    assert.equal(reloadedSnap.dirty, false);
    assert.equal(reloadedSnap.hellWaypointCount, expectedWpCount);
  } finally {
    await driver.close();
  }
});

test("Closed-loop scenario 3: focusedInput draft isolation, Rust boundary rejection, and persisted character edits", async () => {
  const driver = createClosedLoopDriver();
  try {
    const loadRes = await driver.requestHost({
      cmd: "load_fixture_path",
      path: "tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s",
    });
    assert.equal(loadRes.resp.ok, true);
    const initialLevel = loadRes.snap.charLevel;
    const initialItems = loadRes.snap.itemCount;

    // 1. Keystroke draft edits stay strictly inside Elm (`NoOut`) until commit
    const focused = await driver.stepCommand({
      kind: "FocusField",
      key: "level",
      initial: String(initialLevel),
    });
    assert.equal(focused.emittedIntents.length, 0);
    assert.equal(focused.snap.focusedInputKey, "level");

    const typedInvalid = await driver.stepCommand({ kind: "EditFocusedInput", raw: "120" });
    assert.equal(typedInvalid.emittedIntents.length, 0);
    assert.equal(typedInvalid.snap.focusedInputRaw, "120");
    assert.equal(typedInvalid.snap.dirty, false);

    // 2. Commit out-of-range level (120) -> Rust rejects cleanly & keeps session clean
    const rejectedLevel = await driver.stepCommand({ kind: "CommitFocusedInput" });
    assert.deepEqual(rejectedLevel.emittedIntents, [{ SetCharacterField: { field: "Level", value: 120 } }]);
    assert.equal(rejectedLevel.resp.ok, false);
    assert.equal(rejectedLevel.snap.rejectionCode, "invalid_character_mutation");
    assert.equal(rejectedLevel.snap.dirty, false);
    assert.equal(rejectedLevel.snap.charLevel, initialLevel);
    assert.equal(rejectedLevel.snap.focusedInputKey, null);
    await driver.stepCommand({ kind: "ClearBanner" });

    // 3. CancelFocusedInput discards draft without IPC
    await driver.stepCommand({ kind: "FocusField", key: "strength", initial: String(loadRes.snap.strengthBase) });
    await driver.stepCommand({ kind: "EditFocusedInput", raw: "999" });
    const canceled = await driver.stepCommand({ kind: "CancelFocusedInput" });
    assert.equal(canceled.emittedIntents.length, 0);
    assert.equal(canceled.snap.focusedInputKey, null);
    assert.equal(canceled.snap.dirty, false);

    // 4. Commit valid character profile edits (level=50, strength=200, gold_stash=1234567, char_name=OrthoDruid)
    for (const [key, raw] of [
      ["level", "50"],
      ["strength", "200"],
      ["gold_stash", "1234567"],
      ["char_name", "OrthoDruid"],
    ]) {
      await driver.stepCommand({ kind: "FocusField", key, initial: raw });
      await driver.stepCommand({ kind: "EditFocusedInput", raw });
      const committed = await driver.stepCommand({ kind: "CommitFocusedInput" });
      assert.equal(committed.resp.ok, true, `Commit failed for ${key}: ${committed.resp.error?.message}`);
      assert.equal(committed.snap.dirty, true);
    }

    // 5. Gold exceeding dynamic schema cap (level 50 * 10000 = 500000) is rejected by Rust
    await driver.stepCommand({ kind: "FocusField", key: "gold_character", initial: "600000" });
    const rejectedGold = await driver.stepCommand({ kind: "CommitFocusedInput" });
    assert.equal(rejectedGold.resp.ok, false);
    assert.equal(rejectedGold.snap.rejectionCode, "invalid_character_mutation");
    assert.equal(rejectedGold.snap.hasToast, true);
    assert.equal(rejectedGold.snap.toastRawCode, "invalid_character_mutation");

    // Dismiss the rejection toast (leaving valid edits intact)
    const dismissed = await driver.stepCommand({ kind: "DismissToast" });
    assert.equal(dismissed.snap.hasToast, false);

    // 6. Export checksum-valid .d2s and reload into fresh Rust session to verify true roundtrip fidelity
    const { snap: reloadedSnap } = await driver.exportAndReload("scenario3-character-loopback.d2s");
    assert.equal(reloadedSnap.dirty, false);
    assert.equal(reloadedSnap.charLevel, 50);
    assert.equal(reloadedSnap.strengthBase, 200);
    assert.equal(reloadedSnap.goldStash, 1234567);
    assert.equal(reloadedSnap.charName, "OrthoDruid");
    assert.equal(reloadedSnap.itemCount, initialItems);
  } finally {
    await driver.close();
  }
});
