import React, { useEffect, useState } from "react";
import { commands, GameSettings } from "@/bindings";

function PingStartOption({
    pingStartType,
    settings,
    setSettings
}: {
    pingStartType: string;
    settings: GameSettings;
    setSettings: React.Dispatch<React.SetStateAction<GameSettings>>;
}) {
    const [players, setPlayers] = useState(1);
    const [minutes, setMinutes] = useState(1);

    useEffect(() => {
        switch (pingStartType) {
            case "Players":
                setSettings({ ...settings, ping_start: { Players: players } });
                break;

            case "Minutes":
                setSettings({ ...settings, ping_start: { Minutes: minutes } });
                break;

            case "Instant":
                setSettings({ ...settings, ping_start: "Instant" });
                break;
        }
    }, [pingStartType, players, minutes]);

    switch (pingStartType) {
        case "Players":
            return (
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1 m-2"
                    value={players}
                    onChange={(e) => {
                        setPlayers(Number(e.target.value));
                        setSettings({
                            ...settings,
                            ping_start: { Players: players }
                        });
                    }}
                />
            );

        case "Minutes":
            return (
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1 m-2"
                    value={minutes}
                    onChange={(e) => setMinutes(Number(e.target.value))}
                />
            );

        case "Instant":
            return <input disabled={true} className="input-field px-1 py-1 m-2" value="Instant" />;
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
        <div className="flex flex-col items-center max-w-screen max-h-full overflow-y-scroll justify-start p-10 pb-50">
            <div className="setting-option">
                <div className="setting-label">Random Seed</div>
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1 m-2"
                    placeholder={settings.random_seed}
                    onChange={(e) => {
                        setSettings({ ...settings, random_seed: Number(e.target.value) });
                    }}
                />
            </div>

            <div className="setting-option">
                <div className="setting-label">Hiding time</div>
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1 m-2"
                    placeholder={settings.hiding_time_seconds}
                    onChange={(e) => {
                        setSettings({ ...settings, hiding_time_seconds: Number(e.target.value) });
                    }}
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

                    <PingStartOption
                        pingStartType={pingStartType}
                        settings={settings}
                        setSettings={setSettings}
                    />
                </div>
            </div>

            <div className="setting-option">
                <div className="setting-label">Ping Minute Interval</div>
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1 m-2"
                    placeholder={settings.ping_minutes_interval}
                    onChange={(e) => {
                        setSettings({ ...settings, ping_minutes_interval: Number(e.target.value) });
                    }}
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
                    onChange={(e) => {
                        setSettings({ ...settings, powerup_chance: Number(e.target.value) });
                    }}
                />
            </div>

            <div className="setting-option">
                <div className="setting-label">Powerup Minute Cooldown</div>
                <input
                    type="number"
                    min="1"
                    className="input-field px-1 py-1 m-2"
                    placeholder={settings.powerup_minutes_cooldown}
                    onChange={(e) => {
                        setSettings({
                            ...settings,
                            powerup_minutes_cooldown: Number(e.target.value)
                        });
                    }}
                />
            </div>

            <button
                className="btn-blue px-7 py-3"
                onClick={() => {
                    onStartGame(null);
                }}
            >
                Start
            </button>
            <></>
        </div>
    );
}
