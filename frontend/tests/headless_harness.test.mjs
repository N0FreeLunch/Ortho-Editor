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
  assert.equal(loadedSnap.mercPersistenceSupported, true);
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
      assert.equal(exported.ok, true, `Export failed: ${exported.error?.message || JSON.stringify(exported)}`);
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

    // 5. Cycle 18: Mercenary persistence is enabled; AdjustMercLevel +5 mutates and Undo restores
    assert.equal(loadRes.snap.mercPersistenceSupported, true);
    assert.equal(loadRes.snap.exportSupported, true);
    const mercStep = await driver.stepCommand({ kind: "AdjustMercLevel", delta: 5 });
    assert.equal(mercStep.resp.ok, true);
    assert.equal(mercStep.snap.mercLevel, loadRes.snap.mercLevel + 5);
    assert.equal(mercStep.snap.dirty, true);
    const mercUndo = await driver.stepCommand({ kind: "Undo" });
    assert.equal(mercUndo.resp.ok, true);
    assert.equal(mercUndo.snap.mercLevel, loadRes.snap.mercLevel);
    assert.equal(mercUndo.snap.dirty, false);

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

test("Closed-loop scenario 4: Item Studio property inspector & stat mutation engine", async () => {
  const driver = createClosedLoopDriver();
  try {
    const loadRes = await driver.requestHost({
      cmd: "load_fixture_path",
      path: "tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s",
    });
    assert.equal(loadRes.resp.ok, true);
    assert.equal(loadRes.snap.hasSession, true);

    // Switch to View A (Items workbench)
    await driver.stepCommand({ kind: "SwitchView", view: "ViewA" });

    // 1. Select an item (Index 0: cm2 charm)
    const sel = await driver.stepCommand({ kind: "SelectItem", index: 0 });
    assert.equal(sel.snap.selectedItemIndex, 0);

    // 2. Mutate Base Stat (Ethereal flag toggle: false -> true)
    const ethMutate = await driver.stepCommand({
      kind: "MutateItemBase",
      itemIndex: 0,
      defense: null,
      currentDurability: null,
      maxDurability: null,
      quantity: null,
      isEthereal: true,
    });
    assert.equal(ethMutate.resp.ok, true);
    assert.equal(ethMutate.snap.dirty, true);
    assert.equal(ethMutate.snap.canUndo, true);

    // 3. Mutate Item Property (+2 to Dexterity, stat_id = 2, delta = +3 -> 5)
    const propMutate = await driver.stepCommand({
      kind: "MutateItemProperty",
      itemIndex: 0,
      statId: 2,
      delta: 3,
    });
    assert.equal(propMutate.resp.ok, true);
    assert.equal(propMutate.snap.dirty, true);

    // 4. Test Undo functionality: Undo property mutation, then undo ethereal mutation
    const undoProp = await driver.stepCommand({ kind: "Undo" });
    assert.equal(undoProp.resp.ok, true);
    assert.equal(undoProp.snap.canRedo, true);

    const undoEth = await driver.stepCommand({ kind: "Undo" });
    assert.equal(undoEth.resp.ok, true);

    // 5. Redo back both mutations
    const redoEth = await driver.stepCommand({ kind: "Redo" });
    assert.equal(redoEth.resp.ok, true);

    const redoProp = await driver.stepCommand({ kind: "Redo" });
    assert.equal(redoProp.resp.ok, true);

    // 6. Attempt mutating a non-existent stat to verify ActionRejection
    const rejectMutate = await driver.stepCommand({
      kind: "MutateItemProperty",
      itemIndex: 0,
      statId: 9999,
      delta: 1,
    });
    assert.equal(rejectMutate.resp.ok, false);
    assert.equal(rejectMutate.snap.rejectionCode, "stat_not_found_on_item");
    assert.equal(rejectMutate.snap.hasToast, true);

    // 7. Dismiss rejection toast
    const dismissed = await driver.stepCommand({ kind: "DismissToast" });
    assert.equal(dismissed.snap.hasToast, false);

    // 8. Export checksum-valid .d2s and reload into fresh session to verify roundtrip
    const { snap: reloadedSnap } = await driver.exportAndReload("scenario4-item-mutation-loopback.d2s");
    assert.equal(reloadedSnap.dirty, false);
    assert.equal(reloadedSnap.charName, "TESTDRUID");
  } finally {
    await driver.close();
  }
});

test("Closed-loop scenario 5: Mercenary Zone 4 item move, Act switch, Replaced Staging, and .d2s roundtrip", async () => {
  const driver = createClosedLoopDriver();
  try {
    const loadRes = await driver.requestHost({
      cmd: "load_fixture_path",
      path: "tests/fixtures/savegames/gameplay/normal/act3/TESTASSASSIN_Act3_ColdMercLvl21.d2s",
    });
    assert.equal(loadRes.resp.ok, true);
    assert.equal(loadRes.snap.mercAct, 3);
    assert.equal(loadRes.snap.mercPersistenceSupported, true);
    const helm = loadRes.resp.envelope.session.merc_items.find((it) => it.x === 1);
    assert.ok(helm, "Expected mercenary helm in slot 1");

    await driver.stepCommand({ kind: "SwitchView", view: "ViewA" });

    // 1. Move merc helm to Stash (7, 7) and back to MercEquipment slot 1
    const moveOut = await driver.requestHost({
      cmd: "dispatch_intent",
      intent: {
        MoveItem: {
          item_index: helm.index,
          target_container: "Stash",
          x: 7,
          y: 7,
        },
      },
    });
    assert.equal(moveOut.resp.ok, true);
    assert.equal(moveOut.resp.envelope.session.merc_items.length, 1);

    const moveBack = await driver.requestHost({
      cmd: "dispatch_intent",
      intent: {
        MoveItem: {
          item_index: helm.index,
          target_container: "MercEquipment",
          x: 1,
          y: 0,
        },
      },
    });
    assert.equal(moveBack.resp.ok, true);
    assert.equal(moveBack.resp.envelope.session.merc_items.length, 2);

    // 2. Switch Act 3 -> Act 2 via Elm HeadlessHarness command; replaced staging guards export
    const switchAct2 = await driver.stepCommand({ kind: "SelectMercAct", actIndex: 2 });
    assert.equal(switchAct2.resp.ok, true);
    assert.equal(switchAct2.snap.mercAct, 2);
    assert.equal(switchAct2.snap.exportSupported, false);
    assert.equal(switchAct2.resp.envelope.session.merc_replaced_items.length, 2);

    // 2b. Pick up an item from MercReplaced and place it back down into MercReplaced
    const replacedItem = switchAct2.resp.envelope.session.merc_replaced_items[0];
    const pickReplaced = await driver.requestHost({
      cmd: "dispatch_intent",
      intent: { SelectItemForMove: { item_index: replacedItem.index } },
    });
    assert.equal(pickReplaced.resp.ok, true);
    assert.ok(
      pickReplaced.resp.envelope.session.allowed_move_targets.some(
        (t) => t.container === "MercReplaced" && t.x === replacedItem.x && t.y === 0
      ),
      "MercReplaced slot should be in allowed_move_targets"
    );
    const dropReplaced = await driver.requestHost({
      cmd: "dispatch_intent",
      intent: {
        MoveItem: {
          item_index: replacedItem.index,
          target_container: "MercReplaced",
          x: replacedItem.x,
          y: 0,
        },
      },
    });
    assert.equal(dropReplaced.resp.ok, true);
    assert.equal(dropReplaced.resp.envelope.session.merc_replaced_items.length, 2);
    assert.equal(dropReplaced.resp.envelope.session.selected_move_item_index, null);

    // 3. Restore Replaced Mercenary swaps Act 3 and its 2 items back
    const restored = await driver.stepCommand({ kind: "RestoreReplacedMerc" });
    assert.equal(restored.resp.ok, true);
    assert.equal(restored.snap.mercAct, 3);
    assert.equal(restored.snap.exportSupported, true);
    assert.equal(restored.resp.envelope.session.merc_items.length, 2);

    // 4. Switch Act 3 -> Act 1, then Reclaim & Clear replaced staging
    const switchAct1 = await driver.stepCommand({ kind: "SelectMercAct", actIndex: 1 });
    assert.equal(switchAct1.resp.ok, true);
    assert.equal(switchAct1.snap.exportSupported, false);

    const cleared = await driver.stepCommand({ kind: "ClearReplacedMerc", reclaimToStorage: true });
    assert.equal(cleared.resp.ok, true);
    assert.equal(cleared.snap.exportSupported, true);
    assert.equal(cleared.snap.mercAct, 1);

    // 5. Export .d2s and reload into fresh session
    const { snap: reloadedSnap } = await driver.exportAndReload("scenario5-merc-roundtrip.d2s");
    assert.equal(reloadedSnap.dirty, false);
    assert.equal(reloadedSnap.mercAct, 1);
  } finally {
    await driver.close();
  }
});

test("Closed-loop scenario 6: Cycle 19 Hover keyboard shortcuts (Space/S/Escape), Duplicate, Delete, Undo, and .d2s roundtrip", async () => {
  const driver = createClosedLoopDriver();
  try {
    const loadRes = await driver.requestHost({
      cmd: "load_fixture_path",
      path: "tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s",
    });
    assert.equal(loadRes.resp.ok, true);
    const initialCount = loadRes.snap.itemCount;
    const invItem = loadRes.resp.envelope.session.items.find(
      (it) => it.container === "Inventory" && it.width === 1 && it.height === 1,
    );
    assert.ok(invItem, "Expected 1x1 inventory item");

    await driver.stepCommand({ kind: "SwitchView", view: "ViewA" });

    // 1. Hover item and press Space to pick up, then Escape to cancel
    await driver.stepCommand({ kind: "HoverItem", index: invItem.index });
    const picked = await driver.stepCommand({
      kind: "KeyShortcut",
      key: " ",
      code: "Space",
      ctrlKey: false,
      altKey: false,
      metaKey: false,
      isInput: false,
    });
    assert.equal(picked.resp.ok, true);
    assert.equal(picked.resp.envelope.session.selected_move_item_index, invItem.index);

    const cancelled = await driver.stepCommand({
      kind: "KeyShortcut",
      key: "Escape",
      code: "Escape",
      ctrlKey: false,
      altKey: false,
      metaKey: false,
      isInput: false,
    });
    assert.equal(cancelled.resp.ok, true);
    assert.equal(cancelled.resp.envelope.session.selected_move_item_index, null);

    // 2. Press 's' while hovering to quick-move to Stash
    const quickStash = await driver.stepCommand({
      kind: "KeyShortcut",
      key: "s",
      code: "KeyS",
      ctrlKey: false,
      altKey: false,
      metaKey: false,
      isInput: false,
    });
    assert.equal(quickStash.resp.ok, true);
    assert.equal(quickStash.snap.dirty, true);
    const movedCard = quickStash.resp.envelope.session.items.find((it) => it.index === invItem.index);
    assert.equal(movedCard.container, "Stash");

    // 3. Duplicate item -> itemCount + 1
    const dup = await driver.stepCommand({ kind: "ContextDuplicateItem", itemIndex: invItem.index });
    assert.equal(dup.resp.ok, true);
    assert.equal(dup.snap.itemCount, initialCount + 1);
    const clonedIdx = dup.resp.envelope.session.items.at(-1).index;

    // 4. Delete cloned item -> itemCount back to initial, then Undo restores it
    const del = await driver.stepCommand({ kind: "ContextDeleteItem", itemIndex: clonedIdx });
    assert.equal(del.resp.ok, true);
    assert.equal(del.snap.itemCount, initialCount);

    const undone = await driver.stepCommand({ kind: "Undo" });
    assert.equal(undone.resp.ok, true);
    assert.equal(undone.snap.itemCount, initialCount + 1);

    // 5. Export .d2s and reload
    const { snap: reloadedSnap } = await driver.exportAndReload("scenario6-cycle19-roundtrip.d2s");
    assert.equal(reloadedSnap.dirty, false);
    assert.equal(reloadedSnap.itemCount, initialCount + 1);
  } finally {
    await driver.close();
  }
});

test("Closed-loop scenario 7 (Cycle 20): Item Option Editor Modal AddItemProperty / RemoveItemProperty / Undo / .d2s roundtrip", async () => {
  const driver = createClosedLoopDriver();
  try {
    const loadRes = await driver.requestHost({
      cmd: "load_fixture_path",
      path: "tests/fixtures/savegames/gameplay/normal/act3/TESTASSASSIN_Act3_ColdMercLvl21.d2s",
    });
    assert.equal(loadRes.resp.ok, true);
    const session0 = loadRes.resp.envelope.session;
    assert.ok(session0.stat_catalog.length > 50, "stat_catalog should be populated from d2r-data");

    const targetItem = session0.items.find((it) => it.detailed_properties.length > 0);
    assert.ok(targetItem, "need a non-compact item");
    const chosenStat = session0.stat_catalog.find(
      (s) => s.param_bits === 0 && !targetItem.detailed_properties.some((p) => p.stat_id === s.stat_id) && s.max_value >= 15
    );
    assert.ok(chosenStat, "need an unused writable stat");
    const paramStat = session0.stat_catalog.find(
      (s) => s.param_bits >= 4 && !targetItem.detailed_properties.some((p) => p.stat_id === s.stat_id) && s.max_value >= 2 && s.min_value <= 2
    );
    assert.ok(paramStat, "need a parameterized stat in catalog");

    // 1. AddItemProperty via HeadlessHarness
    const addRes = await driver.stepCommand({
      kind: "AddItemProperty",
      itemIndex: targetItem.index,
      statId: chosenStat.stat_id,
      value: 15,
    });
    assert.equal(addRes.resp.ok, true);
    assert.equal(addRes.snap.dirty, true);
    const updatedItem = addRes.resp.envelope.session.items.find((it) => it.index === targetItem.index);
    assert.ok(updatedItem.detailed_properties.some((p) => p.stat_id === chosenStat.stat_id && p.value === 15));

    // 2. RemoveItemProperty + Undo restores it
    const remRes = await driver.stepCommand({
      kind: "RemoveItemProperty",
      itemIndex: targetItem.index,
      statId: chosenStat.stat_id,
    });
    assert.equal(remRes.resp.ok, true);
    const removedItem = remRes.resp.envelope.session.items.find((it) => it.index === targetItem.index);
    assert.ok(!removedItem.detailed_properties.some((p) => p.stat_id === chosenStat.stat_id));

    const undoRes = await driver.stepCommand({ kind: "Undo" });
    assert.equal(undoRes.resp.ok, true);

    // 2b. Cycle 22 parameterized stat injection (param_bits > 0) + SetItemPropertyParam
    const addParamRes = await driver.stepCommand({
      kind: "AddItemProperty",
      itemIndex: targetItem.index,
      statId: paramStat.stat_id,
      value: 2,
      param: 7,
    });
    assert.equal(addParamRes.resp.ok, true);

    const setParamRes = await driver.stepCommand({
      kind: "SetItemPropertyParam",
      itemIndex: targetItem.index,
      statId: paramStat.stat_id,
      param: 12,
    });
    assert.equal(setParamRes.resp.ok, true);

    // 3. Export .d2s and reload
    const { reloaded } = await driver.exportAndReload("scenario7-cycle20-roundtrip.d2s");
    const reloadedItem = reloaded.envelope.session.items.find((it) => it.index === targetItem.index);
    assert.ok(reloadedItem.detailed_properties.some((p) => p.stat_id === chosenStat.stat_id && p.value === 15));
    assert.ok(reloadedItem.detailed_properties.some((p) => p.stat_id === paramStat.stat_id && p.value === 2 && p.param === 12));
  } finally {
    await driver.close();
  }
});

test("Closed-loop scenario 8 (Cycle 21): View A Gold Stash/Inventory edit, negative gold rejection, and .d2s roundtrip", async () => {
  const driver = createClosedLoopDriver();
  try {
    const loadRes = await driver.requestHost({
      cmd: "load_fixture_path",
      path: "tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s",
    });
    assert.equal(loadRes.resp.ok, true);
    const session0 = loadRes.resp.envelope.session;
    assert.equal(session0.merc.persistence_supported, true);
    for (const item of [...session0.items, ...session0.merc_items]) {
      assert.ok(Array.isArray(item.socketed_items));
      assert.equal(item.socketed_items.length, item.socketed_names.length);
      assert.ok(item.socket_count >= item.socketed_items.length);
    }

    // 1. Negative gold triggers Rust ActionRejection without mutating state
    await driver.stepCommand({ kind: "FocusField", key: "gold_stash", initial: String(session0.stats.gold_stash) });
    await driver.stepCommand({ kind: "EditFocusedInput", raw: "-50" });
    const negRes = await driver.stepCommand({ kind: "CommitFocusedInput" });
    assert.equal(negRes.resp.ok, false);
    assert.equal(negRes.snap.rejectionCode, "invalid_character_mutation");

    // 2. Valid gold_stash + gold_character edits persist through .d2s export/reload
    await driver.stepCommand({ kind: "FocusField", key: "gold_stash", initial: String(session0.stats.gold_stash) });
    await driver.stepCommand({ kind: "EditFocusedInput", raw: "888888" });
    const stashRes = await driver.stepCommand({ kind: "CommitFocusedInput" });
    assert.equal(stashRes.resp.ok, true);

    await driver.stepCommand({ kind: "FocusField", key: "gold_character", initial: String(session0.stats.gold_inventory) });
    await driver.stepCommand({ kind: "EditFocusedInput", raw: "44444" });
    const invRes = await driver.stepCommand({ kind: "CommitFocusedInput" });
    assert.equal(invRes.resp.ok, true);

    const { reloaded } = await driver.exportAndReload("scenario8-cycle21-roundtrip.d2s");
    assert.equal(reloaded.envelope.session.stats.gold_stash, 888888);
    assert.equal(reloaded.envelope.session.stats.gold_inventory, 44444);
  } finally {
    await driver.close();
  }
});

test("Closed-loop scenario 9 (Cycle 23): Item Forge Draft staging, Discard isolation, Apply 1-step Undo/Redo, and .d2s roundtrip", async () => {
  const driver = createClosedLoopDriver();
  try {
    const loadRes = await driver.requestHost({
      cmd: "load_fixture_path",
      path: "tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s",
    });
    assert.equal(loadRes.resp.ok, true);
    const session0 = loadRes.resp.envelope.session;
    const targetItem = session0.items.find(
      (it) => !it.is_ear && !it.is_simple && it.detailed_properties.length > 0
    );
    assert.ok(targetItem, "Expected an editable non-simple item");

    const existingStatIds = new Set(targetItem.detailed_properties.map((p) => p.stat_id));
    const chosenStat = session0.stat_catalog.find(
      (s) => !existingStatIds.has(s.stat_id) && s.param_bits === 0 && s.min_value <= 15 && s.max_value >= 15
    );
    assert.ok(chosenStat, "Expected an unused 0-param stat in catalog");

    // 1. Open draft and add property -> staged in draft only, main item & undo_stack untouched
    const open1 = await driver.stepCommand({ kind: "OpenItemForgeDraft", itemIndex: targetItem.index });
    assert.equal(open1.resp.ok, true);
    assert.equal(open1.resp.envelope.session.active_item_draft.dirty, false);

    const add1 = await driver.stepCommand({
      kind: "AddItemProperty",
      itemIndex: targetItem.index,
      statId: chosenStat.stat_id,
      value: 15,
      param: 0,
    });
    assert.equal(add1.resp.ok, true);
    assert.equal(add1.resp.envelope.session.active_item_draft.dirty, true);
    assert.ok(
      add1.resp.envelope.session.active_item_draft.card.detailed_properties.some(
        (p) => p.stat_id === chosenStat.stat_id && p.value === 15
      )
    );
    const mainDuringDraft = add1.resp.envelope.session.items.find((it) => it.index === targetItem.index);
    assert.ok(!mainDuringDraft.detailed_properties.some((p) => p.stat_id === chosenStat.stat_id));
    assert.equal(add1.resp.envelope.session.can_undo, false);

    // 2. Discard draft -> clears draft, main item & undo_stack remain untouched
    const discardRes = await driver.stepCommand({ kind: "DiscardItemForgeDraft" });
    assert.equal(discardRes.resp.ok, true);
    assert.equal(discardRes.resp.envelope.session.active_item_draft, null);
    assert.equal(discardRes.resp.envelope.session.can_undo, false);
    const mainAfterDiscard = discardRes.resp.envelope.session.items.find((it) => it.index === targetItem.index);
    assert.ok(!mainAfterDiscard.detailed_properties.some((p) => p.stat_id === chosenStat.stat_id));

    // 3. Re-open draft, add property, Apply -> commits atomically in 1 undo step
    await driver.stepCommand({ kind: "OpenItemForgeDraft", itemIndex: targetItem.index });
    await driver.stepCommand({
      kind: "AddItemProperty",
      itemIndex: targetItem.index,
      statId: chosenStat.stat_id,
      value: 15,
      param: 0,
    });
    const applyRes = await driver.stepCommand({ kind: "ApplyItemForgeDraft" });
    assert.equal(applyRes.resp.ok, true);
    assert.equal(applyRes.resp.envelope.session.active_item_draft, null);
    assert.equal(applyRes.resp.envelope.session.can_undo, true);
    const mainAfterApply = applyRes.resp.envelope.session.items.find((it) => it.index === targetItem.index);
    assert.ok(mainAfterApply.detailed_properties.some((p) => p.stat_id === chosenStat.stat_id && p.value === 15));

    // 4. Single Undo reverts, single Redo restores
    const undoRes = await driver.stepCommand({ kind: "Undo" });
    assert.equal(undoRes.resp.ok, true);
    assert.equal(undoRes.resp.envelope.session.can_undo, false);
    assert.equal(undoRes.resp.envelope.session.can_redo, true);
    const mainAfterUndo = undoRes.resp.envelope.session.items.find((it) => it.index === targetItem.index);
    assert.ok(!mainAfterUndo.detailed_properties.some((p) => p.stat_id === chosenStat.stat_id));

    const redoRes = await driver.stepCommand({ kind: "Redo" });
    assert.equal(redoRes.resp.ok, true);
    const mainAfterRedo = redoRes.resp.envelope.session.items.find((it) => it.index === targetItem.index);
    assert.ok(mainAfterRedo.detailed_properties.some((p) => p.stat_id === chosenStat.stat_id && p.value === 15));

    // 5. Export .d2s and reload
    const { reloaded } = await driver.exportAndReload("scenario9-cycle23-roundtrip.d2s");
    const reloadedItem = reloaded.envelope.session.items.find((it) => it.index === targetItem.index);
    assert.ok(reloadedItem.detailed_properties.some((p) => p.stat_id === chosenStat.stat_id && p.value === 15));
  } finally {
    await driver.close();
  }
});
