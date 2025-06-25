import React, { Dispatch, SetStateAction } from "react";
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
                className="nav-btn text-blue-500"
                onClick={() => {
                    setCurrent(target);
                }}
            >
                {children}
            </button>
        );

    return (
        <button
            className="nav-btn text-gray-500"
            onClick={() => {
                setCurrent(target);
            }}
        >
            {children}
        </button>
    );
}
