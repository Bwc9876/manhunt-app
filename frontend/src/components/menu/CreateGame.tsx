import React, { useEffect, useState } from "react";
import { commands, GameSettings, PingStartCondition } from "@/bindings";
import { start } from "repl";

function PingStartOption({
    type,
    startCondition,
    setStartCondition
}: {
    type: string,
    startCondition: PingStartCondition;
    setStartCondition: React.Dispatch<React.SetStateAction<PingStartCondition>>;
}) {
    const [players, setPlayers] = useState(1);
    const [minutes, setMinutes] = useState(1);

    useEffect(() => {
        switch (type) {
            case "Players":
                setStartCondition({ Players: players });
                break;
            
            case "Minutes":
                setStartCondition({ Minutes: minutes });
                break;

            case "Instant":
                setStartCondition('Instant');
                break;
        }
    }, [type, players, minutes]);

    switch (type) {
        case "Players":
            return <input
                type="number"
                min="1"
                className="input-field px-1 py-1 m-2"
                value={players}
                onChange={(e) => setPlayers(Number(e.target.value))}
            />
            
        case "Minutes":
            return <input
                type="number"
                min="1"
                className="input-field px-1 py-1 m-2"
                value={minutes}
                onChange={(e) => setMinutes(Number(e.target.value))}
            />

        case "Instant":
            return <input disabled={true} className="input-field px-1 py-1 m-2" value="Instant" />;
    }
    
    return <></>;
}

function StartSelectionMenu({
    label,
    startCondition,
    setStartCondition
}: {
    label: string;
    startCondition: PingStartCondition,
    setStartCondition: React.Dispatch<React.SetStateAction<PingStartCondition>>;
}) {
    const [type, setType] = useState('Instant');
    
    return (
        <div className="setting-option">
            <div className="setting-label">{label}</div>
            <div className="flex flex-row justify-between items-center-safe">
                <select
                    className="input-field px-5 py-2"
                    value={type}
                    onChange={(e) => setType(e.target.value)}
                >
                    <option selected={startCondition.Players !== undefined} value="Players">
                        Players
                    </option>
                    
                    <option selected={startCondition.Minutes !== undefined} value="Minutes">
                        Minutes
                    </option>
                    
                    <option selected={startCondition === "Instant"} value="Instant">
                        Instant
                    </option>
                </select>

                {<PingStartOption type={type} startCondition={startCondition} setStartCondition={setStartCondition} />}
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

    useEffect(() => {
        console.log(settings);
    }, [settings]);

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

            <StartSelectionMenu
                label="Ping Start"
                startCondition={pingStartCondition}
                setStartCondition={setPingStartCondition}
            />

            <StartSelectionMenu
                label="Powerup Start"
                startCondition={powerupStartCondition}
                setStartCondition={setPowerupStartCondition}
            />

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
            
        </div>
    );
}
