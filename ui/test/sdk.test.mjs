import test from "node:test";
import assert from "node:assert/strict";
import { FiexBridgeError, FiexUiClient, fiex } from "../dist/index.js";

test("UI SDK exports independent bridge client", () => {
  assert.ok(fiex instanceof FiexUiClient);
  const err = new FiexBridgeError("permission_denied", "denied");
  assert.equal(err.code, "permission_denied");
  assert.equal(err.name, "FiexBridgeError");
  assert.equal(err.message, "denied");
});

import { attachFiexPanel } from "../dist/host.js";

test("host transfers a port only after the client listens, supports events and rejects unauthorized calls", async () => {
  const previous = globalThis.window;
  const hostWindow = new EventTarget();
  hostWindow.location = { origin: "https://host" };
  globalThis.window = hostWindow;
  let peer;
  let ready;
  const didReady = new Promise((resolve) => {
    ready = resolve;
  });
  const frameWindow = {
    postMessage(message, _origin, ports) {
      if (message.type === "fiex.bridge.probe") {
        const event = new Event("message");
        Object.assign(event, {
          source: frameWindow,
          data: { type: "fiex.bridge.listening" },
        });
        hostWindow.dispatchEvent(event);
      } else {
        peer = ports[0];
        peer.postMessage({
          type: "fiex.bridge.ready",
          bridgeVersion: "1.0",
          sessionId: message.sessionId,
          nonce: message.nonce,
        });
      }
    },
  };
  let emit;
  let unsubscribed = false;
  const dispose = attachFiexPanel({
    pluginId: "test",
    sessionId: "session",
    origin: "null",
    frame: { contentWindow: frameWindow },
    allowedMethods: new Set(["echo"]),
    dispatch: async (_method, p) => p,
    onReady: ready,
    subscribe: (send) => {
      emit = send;
      return () => {
        unsubscribed = true;
      };
    },
  });
  try {
    await didReady;
    const response = new Promise((resolve) => {
      peer.onmessage = (e) => resolve(e.data);
    });
    peer.postMessage({
      type: "request",
      bridgeVersion: "1.0",
      sessionId: "session",
      requestId: "1",
      method: "forbidden",
    });
    assert.equal((await response).error.code, "permission_denied");
    const event = new Promise((resolve) => {
      peer.onmessage = (e) => resolve(e.data);
    });
    emit("theme.changed", { mode: "dark" });
    assert.equal((await event).name, "theme.changed");
  } finally {
    dispose();
    peer?.close();
    globalThis.window = previous;
  }
  assert.equal(unsubscribed, true);
});

test("disposed client fails promptly without starting another handshake", async () => {
  const client = new FiexUiClient();
  client.dispose();
  await assert.rejects(client.ready(), (error) => error.code === "closed");
});
