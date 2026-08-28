import assert from "node:assert/strict";
import test from "node:test";
import {
  DashboardConflictError,
  isRevisionConflict,
  requestV3,
  setControlRevisionSink,
  type ControlPlaneTokens,
} from "./dashboard-v3.ts";

test("Dashboard V3 transport maps revisionConflict and publishes fresh CAS tokens", async (context) => {
  const windowDescriptor = Object.getOwnPropertyDescriptor(globalThis, "window");
  const fetchDescriptor = Object.getOwnPropertyDescriptor(globalThis, "fetch");
  context.after(() => {
    setControlRevisionSink(null);
    if (windowDescriptor) Object.defineProperty(globalThis, "window", windowDescriptor);
    else Reflect.deleteProperty(globalThis, "window");
    if (fetchDescriptor) Object.defineProperty(globalThis, "fetch", fetchDescriptor);
    else Reflect.deleteProperty(globalThis, "fetch");
  });

  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      location: { pathname: "/dashboard/" },
      dispatchEvent: () => true,
    },
  });
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async () => new Response(JSON.stringify({
      code: "revisionConflict",
      message: "stale mutation",
      currentRevision: 42,
      processGeneration: 7,
    }), {
      status: 409,
      headers: { "Content-Type": "application/json" },
    }),
  });

  let published: ControlPlaneTokens | null = null;
  setControlRevisionSink((tokens) => { published = tokens; });

  await assert.rejects(
    requestV3("/settings"),
    (error: unknown) => {
      assert.ok(error instanceof DashboardConflictError);
      assert.equal(error.code, "revisionConflict");
      assert.equal(error.currentRevision, 42);
      assert.equal(error.processGeneration, 7);
      assert.ok(isRevisionConflict(error));
      return true;
    },
  );
  assert.deepEqual(published, { revision: 42, processGeneration: 7, pricingRevision: null });
});
