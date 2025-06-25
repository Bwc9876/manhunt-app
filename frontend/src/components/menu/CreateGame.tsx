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
        <div className="flex flex-col items-center justify-left p-3.5">
            <div className="setting-option">
                <div className="setting-label">Random Seed</div>
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1"
                    placeholder={settings.random_seed}
                ></input>
            </div>

            <div className="setting-option">
                <div className="setting-label">Hiding time</div>
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1"
                    placeholder={settings.hiding_time_seconds}
                ></input>
            </div>

            <div className="setting-option">
                <div className="setting-label">Powerup Type</div>
                <select className="input-field px-1 py-1">
                    <option value="PingSeeker">Ping Seeker</option>
                    <option value="PingAllSeekers">Ping All Seekers</option>
                    <option value="ForcePingOther">Force Ping Others</option>
                </select>
            </div>
        </div>
    );
}
