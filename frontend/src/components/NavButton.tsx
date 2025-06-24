import { commands, GameSettings } from "@/bindings";
import { sharedSwrConfig } from "@/lib/hooks";
import React, { Dispatch, SetStateAction } from "react";
import useSWR from "swr";
import { MenuState } from "./MenuScreen";

export default function NavButton({
    label,
    current,
    setCurrent,
    target
}: {
    label: string;
    current: MenuState;
    setCurrent: Dispatch<SetStateAction<MenuState>>;
    target: MenuState;
}) {
    if (current == target)
        return (
            <button
                className="bg-transparent text-xs text-center text-blue-400 p-2.5 grow"
                onClick={() => {
                    setCurrent(target);
                }}
            >
                {label}
            </button>
        );

    return (
        <button
            className="bg-transparent text-xs text-center text-gray-600 p-2.5 grow"
            onClick={() => {
                setCurrent(target);
            }}
        >
            {label}
        </button>
    );
}
