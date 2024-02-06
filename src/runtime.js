// runtime.js

// deno_webidl
import * as webidl from "ext:deno_webidl/00_webidl.js";

// deno_console
import * as console from "ext:deno_console/01_console.js";

// deno_web
import { DOMException } from "ext:deno_web/01_dom_exception.js";
import * as timers from "ext:deno_web/02_timers.js";
import * as abortSignal from "ext:deno_web/03_abort_signal.js";
import {} from "ext:deno_web/04_global_interfaces.js";
import * as base64 from "ext:deno_web/05_base64.js";
import * as streams from "ext:deno_web/06_streams.js";
import * as encoding from "ext:deno_web/08_text_encoding.js";
import * as file from "ext:deno_web/09_file.js";
import * as fileReader from "ext:deno_web/10_filereader.js";
import * as location from "ext:deno_web/12_location.js";
import * as messagePort from "ext:deno_web/13_message_port.js";
import * as compression from "ext:deno_web/14_compression.js";
import * as performance from "ext:deno_web/15_performance.js";

// deno_url
import * as url from "ext:deno_url/00_url.js";
import * as urlPattern from "ext:deno_url/01_urlpattern.js";

import { core, primordials } from "ext:core/mod.js";

{
  core.print(`Will setup runtime.js\n`);

  const { ObjectDefineProperties, ObjectDefineProperty, SymbolFor } =
    primordials;

  class WorkerNavigator {
    constructor() {
      webidl.illegalConstructor();
    }

    [SymbolFor("Deno.privateCustomInspect")](inspect) {
      return `${this.constructor.name} ${inspect({})}`;
    }
  }

  const workerNavigator = webidl.createBranded(WorkerNavigator);

  let numCpus, userAgent, language;

  // https://developer.mozilla.org/en-US/docs/Web/API/WorkerNavigator
  ObjectDefineProperties(WorkerNavigator.prototype, {
    hardwareConcurrency: {
      configurable: true,
      enumerable: true,
      get() {
        webidl.assertBranded(this, WorkerNavigatorPrototype);
        return numCpus;
      },
    },
    userAgent: {
      configurable: true,
      enumerable: true,
      get() {
        webidl.assertBranded(this, WorkerNavigatorPrototype);
        return userAgent;
      },
    },
    language: {
      configurable: true,
      enumerable: true,
      get() {
        webidl.assertBranded(this, WorkerNavigatorPrototype);
        return language;
      },
    },
    languages: {
      configurable: true,
      enumerable: true,
      get() {
        webidl.assertBranded(this, WorkerNavigatorPrototype);
        return [language];
      },
    },
  });

  const WorkerNavigatorPrototype = WorkerNavigator.prototype;

  function nonEnumerable(value) {
    return {
      value,
      writable: true,
      enumerable: false,
      configurable: true,
    };
  }

  function writable(value) {
    return {
      value,
      writable: true,
      enumerable: true,
      configurable: true,
    };
  }

  function readOnly(value) {
    return {
      value,
      enumerable: true,
      writable: false,
      configurable: true,
    };
  }

  function getterOnly(getter) {
    return {
      get: getter,
      set() {},
      enumerable: true,
      configurable: true,
    };
  }

  // https://developer.mozilla.org/en-US/docs/Web/API/WorkerGlobalScope
  const windowOrWorkerGlobalScope = {
    console: nonEnumerable(
      // https://choubey.gitbook.io/internals-of-deno/bridge/4.2-print
      new console.Console((msg, level) => core.print(msg, level > 1))
    ),

    // DOM Exception
    // deno_web - 01 - dom_exception
    DOMException: nonEnumerable(DOMException),

    // Timers
    // deno_web - 02 - timers
    clearInterval: writable(timers.clearInterval),
    clearTimeout: writable(timers.clearTimeout),
    setInterval: writable(timers.setInterval),
    setTimeout: writable(timers.setTimeout),

    // Abort signal
    // deno_web - 03 - abort_signal
    AbortController: nonEnumerable(abortSignal.AbortController),
    AbortSignal: nonEnumerable(abortSignal.AbortSignal),

    // Base64
    // deno_web - 05 - base64
    atob: writable(base64.atob),
    btoa: writable(base64.btoa),

    // Streams
    // deno_web - 06 - streams

    // streams
    ByteLengthQueuingStrategy: nonEnumerable(streams.ByteLengthQueuingStrategy),
    CountQueuingStrategy: nonEnumerable(streams.CountQueuingStrategy),
    ReadableStream: nonEnumerable(streams.ReadableStream),
    ReadableStreamDefaultReader: nonEnumerable(
      streams.ReadableStreamDefaultReader
    ),
    ReadableByteStreamController: nonEnumerable(
      streams.ReadableByteStreamController
    ),
    ReadableStreamBYOBReader: nonEnumerable(streams.ReadableStreamBYOBReader),
    ReadableStreamBYOBRequest: nonEnumerable(streams.ReadableStreamBYOBRequest),
    ReadableStreamDefaultController: nonEnumerable(
      streams.ReadableStreamDefaultController
    ),
    TransformStream: nonEnumerable(streams.TransformStream),
    TransformStreamDefaultController: nonEnumerable(
      streams.TransformStreamDefaultController
    ),
    WritableStream: nonEnumerable(streams.WritableStream),
    WritableStreamDefaultWriter: nonEnumerable(
      streams.WritableStreamDefaultWriter
    ),
    WritableStreamDefaultController: nonEnumerable(
      streams.WritableStreamDefaultController
    ),

    // Text Encoding
    // deno_web - 08 - text_encoding
    TextDecoder: nonEnumerable(encoding.TextDecoder),
    TextEncoder: nonEnumerable(encoding.TextEncoder),
    TextDecoderStream: nonEnumerable(encoding.TextDecoderStream),
    TextEncoderStream: nonEnumerable(encoding.TextEncoderStream),

    // File
    // deno_web - 09 - file
    File: nonEnumerable(file.File),
    Blob: nonEnumerable(file.Blob),

    // FileReader
    // deno_web - 10 - filereader
    FileReader: nonEnumerable(fileReader),

    // Compression
    // deno_web - 14 - compression
    CompressionStream: nonEnumerable(compression.CompressionStream),
    DecompressionStream: nonEnumerable(compression.DecompressionStream),

    // Performance
    // deno_web - 15 - performance
    Performance: nonEnumerable(performance.Performance),
    PerformanceEntry: nonEnumerable(performance.PerformanceEntry),
    PerformanceMark: nonEnumerable(performance.PerformanceMark),
    PerformanceMeasure: nonEnumerable(performance.PerformanceMeasure),
    performance: writable(performance.performance),

    // MessagePort
    structuredClone: writable(messagePort.structuredClone),

    // URL
    // deno_url
    URL: nonEnumerable(url.URL),
    URLPattern: nonEnumerable(urlPattern.URLPattern),
    URLSearchParams: nonEnumerable(url.URLSearchParams),
  };

  const globalProperties = {
    WorkerLocation: location.workerLocationConstructorDescriptor,
    location: location.workerLocationDescriptor,
    WorkerNavigator: nonEnumerable(WorkerNavigator),
    navigator: getterOnly(() => workerNavigator),
    self: getterOnly(() => globalThis),
  };

  let hasBootstrapped = false;

  globalThis.bootstrap = () => {
    core.print(`Bootstrapping runtime\n`);

    if (hasBootstrapped) {
      throw new Error("Worker runtime already bootstrapped");
    }

    hasBootstrapped = true;

    // TODO
    numCpus = 1;
    language = "en-US";
    userAgent = "OpenWorkers/0.0.0";

    // Delete globalThis.bootstrap (this function)
    delete globalThis.bootstrap;

    // Delete globalThis.console (from v8)
    delete globalThis.console;

    // delete globalThis.Deno;/
    delete globalThis.__bootstrap;

    // Assign global scope properties
    ObjectDefineProperties(globalThis, windowOrWorkerGlobalScope);

    // Assign global properties
    ObjectDefineProperties(globalThis, globalProperties);

    // Remove Deno from globalThis
    ObjectDefineProperty(globalThis, "Deno", {
      value: undefined,
      writable: true,
      enumerable: false,
      configurable: true,
    });
  };
}