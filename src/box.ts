import {
  Actor,
  ActorSubclass,
  Agent,
  HttpAgent,
  Identity,
} from "@dfinity/agent";
import { Principal } from "@dfinity/principal";
import * as Interface from "../.dfx/local/canisters/box/box.did.js";
export * from "../.dfx/local/canisters/box/box.did.js";

class Lazy<T> {
  readonly #loader: () => Promise<T>;
  #value?: T;
  #inflight?: Promise<T>;

  constructor(loader: () => Promise<T>) {
    this.#loader = loader;
  }

  async get(): Promise<T> {
    if (this.#value !== undefined) {
      return this.#value;
    }

    if (this.#inflight != null) {
      return this.#inflight;
    }

    return (this.#inflight = this.#loader()
      .then((t) => {
        this.#value = t;
        return t;
      })
      .finally(() => {
        this.#inflight = undefined;
      }));
  }
}

const WASM = new Lazy<ArrayBuffer>(async () => {
  const res = await fetch(new URL("./box.wasm", import.meta.url));

  if (!res.ok) {
    throw new Error("Failed to load box canister bytecode");
  }

  return res.arrayBuffer();
});

export type Box = ActorSubclass<Interface._SERVICE> & { id: string };

export class BoxManager {
  readonly #agent: Agent;

  private constructor(agent: Agent) {
    this.#agent = agent;
  }

  static withAgent(agent: Agent) {
    return new BoxManager(agent);
  }

  static withIdentity(identity: Identity | Promise<Identity>) {
    return new BoxManager(new HttpAgent({ identity }));
  }

  async create({
    waitForInstall = true,
  }: { waitForInstall?: boolean } = {}): Promise<Box> {
    const principal = await Actor.createCanister({ agent: this.#agent });
    const callOptions = { agent: this.#agent, canisterId: principal };
    const installPromise = Actor.install(
      {
        module: await WASM.get(),
      },
      callOptions
    );
    if (waitForInstall) {
      await installPromise;
    }
    const actor = Actor.createActor(
      // @ts-expect-error
      Interface.idlFactory,
      callOptions
    );
    return Object.assign(actor as unknown as Box, {
      id: principal.toString(),
    });
  }

  connect(canisterId: string | Principal) {
    return Object.assign(
      Actor.createActor<Interface._SERVICE>(
        // @ts-expect-error
        Interface.idlFactory,
        {
          agent: this.#agent,
          canisterId,
        }
      ),
      { id: canisterId.toString() }
    );
  }
}
