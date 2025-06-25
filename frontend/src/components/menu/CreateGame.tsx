import React, { useState } from "react";
import { GameSettings } from "@/bindings";

function PingStartOption({ pingStartType }: { pingStartType: string }) {
    switch (pingStartType) {
        case "Players":
            return (
                <input
                    type="number"
                    min="1"
                    placeholder="Enter number of players"
                    className="input-field px-1 py-1 m-2"
                />
            );

        case "Minutes":
            return (
                <input
                    type="number"
                    min="1"
                    placeholder="Enter minutes"
                    className="input-field px-1 py-1 m-2"
                />
            );

        case "Instant":
            return (
                <input
                    disabled={true}
                    min="1"
                    className="input-field px-1 py-1 m-2"
                    value={"Instant"}
                />
            );
    }

    return <></>;
}
export default function CreateGame({
    settings,
    setSettings
}: {
    settings: GameSettings;
    setSettings: React.Dispatch<React.SetStateAction<GameSettings>>;
}) {
    const [pingStartType, setPingStartType] = useState("Players");

    return (
        <div className="flex flex-col items-center max-w-screen max-h-full overflow-y-scroll justify-start p-10 pb-50">
            <div className="setting-option">
                <div className="setting-label">Random Seed</div>
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1 m-2"
                    placeholder={settings.random_seed}
                />
            </div>

            <div className="setting-option">
                <div className="setting-label">Hiding time</div>
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1 m-2"
                    placeholder={settings.hiding_time_seconds}
                />
            </div>

            <div className="setting-option">
                <div className="setting-label">Ping Start</div>
                <div className="flex flex-row justify-between items-center-safe">
                    <select
                        className="input-field px-5 py-2"
                        onChange={(e) => setPingStartType(e.target.value)}
                    >
                        <option value="Players">Players</option>
                        <option value="Minutes">Minutes</option>
                        <option value="Instant">Instant</option>
                    </select>

                    <PingStartOption pingStartType={pingStartType} />
                </div>
            </div>

            <div className="setting-option">
                <div className="setting-label">Ping Minute Interval</div>
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1 m-2"
                    placeholder={settings.powerup_minutes_cooldown}
                ></input>
            </div>

            <div className="setting-option">
                <div className="setting-label">Powerup Chance</div>
                <input
                    type="number"
                    min="1"
                    max="100"
                    step="0.01"
                    className="input-field px-1 py-1 m-2"
                    placeholder={settings.powerup_chance}
                />
            </div>

            <div className="setting-option">
                <div className="setting-label">Powerup Minute Cooldown</div>
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1 m-2"
                    placeholder={settings.powerup_minutes_cooldown}
                />
            </div>
        </div>
    );
}
