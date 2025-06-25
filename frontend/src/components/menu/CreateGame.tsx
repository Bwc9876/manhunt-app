import React from "react";
import { GameSettings } from "@/bindings";

export default function CreateGame({
    settings,
    setSettings
}: {
    settings: GameSettings;
    setSettings: React.Dispatch<React.SetStateAction<GameSettings>>;
}) {
    return (
        <div className="flex flex-col items-center justify-center p-3.5">
            <div>{settings.random_seed}</div>
        </div>
    );
}
