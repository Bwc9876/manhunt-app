import React, { useEffect, useState } from "react";
import { commands, GameSettings, PingStartCondition } from "@/bindings";
import * as motion from "motion/react-client";

// I am so sorry
function StartSelectionMenu({
    label,
    conditions,
    setStartCondition
}: {
    label: string;
    conditions: PingStartCondition;
    setStartCondition: React.Dispatch<React.SetStateAction<PingStartCondition>>;
}) {
    const players: number | undefined = Object.prototype.hasOwnProperty.call(conditions, "Players")
        ? (conditions as { Players: number }).Players
        : undefined;

    const minutes: number | undefined = Object.prototype.hasOwnProperty.call(conditions, "Minutes")
        ? (conditions as { Minutes: number }).Minutes
        : undefined;

    const changeOption = (e: React.ChangeEvent<HTMLSelectElement>) => {
        switch (e.target.value) {
            case "Players":
                setStartCondition({ Players: players ? players : 4 });
                break;

            case "Minutes":
                setStartCondition({ Minutes: minutes ? minutes : 1 });
                break;

            case "Instant":
                setStartCondition("Instant");
                break;
        }
    };

    return (
        <div className="setting-option w-7/8">
            <div className="setting-label">{label}</div>
            <div className="flex flex-row justify-center items-center w-4/5 m-2.5">
                <select
                    className="input-field p-1.5 w-2/3 mr-3.5"
                    onChange={(e) => changeOption(e)}
                >
                    <option selected={players !== undefined} value="Players">
                        Players
                    </option>
                    <option selected={minutes !== undefined} value="Minutes">
                        Minutes
                    </option>
                    <option selected={conditions === "Instant"} value="Instant">
                        Instant
                    </option>
                </select>

                {players !== undefined && (
                    <input
                        type="number"
                        min="1"
                        className="input-field p-1.5 w-2/3"
                        placeholder={String(players)}
                        onChange={(e) => setStartCondition({ Players: Number(e.target.value) })}
                    />
                )}

                {minutes !== undefined && (
                    <input
                        type="number"
                        min="1"
                        className="input-field p-1.5 w-2/3"
                        placeholder={String(minutes)}
                        onChange={(e) => setStartCondition({ Minutes: Number(e.target.value) })}
                    />
                )}

                {conditions === "Instant" && (
                    <input
                        type="number"
                        min="1"
                        className="input-field p-1.5 w-2/3"
                        placeholder={"Instant"}
                        disabled={true}
                        onChange={() => setStartCondition("Instant")}
                    />
                )}
            </div>
        </div>
    );
}

export default function CreateGame({
    settings,
    setSettings
}: {
    settings: GameSettings;
    setSettings: React.Dispatch<React.SetStateAction<GameSettings>>;
}) {
    const [pingStartCondition, setPingStartCondition] = useState(settings.ping_start);
    const [powerupStartCondition, setPowerupStartCondition] = useState(settings.powerup_start);

    useEffect(() => {
        setSettings({
            ...settings,
            ping_start: pingStartCondition,
            powerup_start: powerupStartCondition
        });
    }, [pingStartCondition, powerupStartCondition]);

    const onStartGame = async (code: string | null) => {
        if (code) {
            try {
                const validCode = await commands.checkRoomCode(code);
                if (!validCode) {
                    window.alert("Invalid Join Code");
                    return;
                }
            } catch (e) {
                window.alert(`Failed to connect to Server ${e}`);
                return;
            }
        }

        await commands.startLobby(code, settings);
    };

    return (
        <motion.div
            className="flex flex-col items-center max-w-screen max-h-full overflow-y-scroll justify-start p-10 pb-30"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
        >
            <h1 className="text-center w-4/3 text-2xl font-semibold mb-4">Game Settings</h1>
            <div className="setting-option w-4/5">
                <div className="setting-label">Random Seed</div>
                <input
                    type="number"
                    min="1"
                    className="input-field w-2/3 p-1.5 m-2"
                    placeholder={String(settings.random_seed)}
                    onChange={(e) => {
                        setSettings({ ...settings, random_seed: Number(e.target.value) });
                    }}
                />
            </div>

            <div className="setting-option w-4/5">
                <div className="setting-label">Hiding time</div>
                <input
                    type="number"
                    min="1"
                    className="input-field w-2/3 p-1.5 m-2"
                    placeholder={String(settings.hiding_time_seconds)}
                    onChange={(e) => {
                        setSettings({
                            ...settings,
                            hiding_time_seconds: Number(e.target.value)
                        });
                    }}
                />
            </div>

            <StartSelectionMenu
                label="Ping Start"
                conditions={settings.ping_start}
                setStartCondition={setPingStartCondition}
            />

            <StartSelectionMenu
                label="Powerup Start"
                conditions={settings.powerup_start}
                setStartCondition={setPowerupStartCondition}
            />

            <div className="setting-option w-4/5">
                <div className="setting-label">Ping Minute Interval</div>
                <input
                    type="number"
                    min="1"
                    className="input-field w-2/3 p-1.5 m-2"
                    placeholder={String(settings.ping_minutes_interval)}
                    onChange={(e) => {
                        setSettings({
                            ...settings,
                            ping_minutes_interval: Number(e.target.value)
                        });
                    }}
                />
            </div>

            <div className="setting-option w-4/5">
                <div className="setting-label">Powerup Chance</div>
                <input
                    type="number"
                    min="1"
                    max="100"
                    step="0.01"
                    className="input-field w-2/3 p-1.5 m-2"
                    placeholder={String(settings.powerup_chance)}
                    onChange={(e) => {
                        setSettings({ ...settings, powerup_chance: Number(e.target.value) });
                    }}
                />
            </div>

            <div className="setting-option w-4/5">
                <div className="setting-label">Powerup Minute Cooldown</div>
                <input
                    type="number"
                    min="1"
                    className="input-field w-2/3 p-1.5 m-2"
                    placeholder={String(settings.powerup_minutes_cooldown)}
                    onChange={(e) => {
                        setSettings({
                            ...settings,
                            powerup_minutes_cooldown: Number(e.target.value)
                        });
                    }}
                />
            </div>

            <button
                className="btn-blue w-1/4 m-2 py-3"
                onClick={() => onStartGame(null)}
            >
                Start
            </button>
        </motion.div>
    );
}
