import { useEffect } from "react";
import { events } from "@/bindings";
import { SWRConfiguration } from "swr";

type ExtractCallback<E extends keyof typeof events> = (
    payload: Parameters<Parameters<(typeof events)[E]["listen"]>[0]>[0]["payload"]
) => void;

type SWRConfigTyp = { suspense: true } & SWRConfiguration;

export const sharedSwrConfig: SWRConfigTyp = { suspense: true, dedupingInterval: 100 };

/**
 *  Convenience hook that does useEffect for a Tauri event and handles unsubscribing on unmount
 */
export function useTauriEvent<E extends keyof typeof events>(
    tauriEvent: E,
    cb: ExtractCallback<E>
) {
    useEffect(() => {
        const unlisten = events[tauriEvent].listen((e) => {
            cb(e.payload);
        });

        return () => {
            unlisten.then((f) => f());
        };
    }, [tauriEvent, cb]);
}
