import { commands, GameSettings } from "@/bindings";
import { sharedSwrConfig } from "@/lib/hooks";
import React, { Dispatch, SetStateAction } from "react";
import useSWR from "swr";
import { MenuState } from "./MenuScreen";

interface NavButtonProps extends React.PropsWithChildren {
    current: MenuState;
    setCurrent: Dispatch<SetStateAction<MenuState>>;
    target: MenuState;
}

export default function NavButton({ current, setCurrent, target, children }: NavButtonProps) {
    if (current == target)
        return (
            <button
                className="flex flex-col bg-transparent text-xs/6 text-center items-center align-middle text-blue-400 p-3.5 grow"
                onClick={() => {
                    setCurrent(target);
                }}
            >
                {children}
            </button>
        );

    return (
        <button
            className="flex flex-col bg-transparent text-xs/6 text-center items-center align-middle text-gray-500 p-3.5 grow"
            onClick={() => {
                setCurrent(target);
            }}
        >
            {children}
        </button>
    );
}
