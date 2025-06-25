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
            <div className="flex flex-row justify-center items-center">
                <div className="text-center mr-2.5">Random Seed</div>
                <input
                    type="number"
                    min="1"
                    className="text-center px-1 py-1 m-5 rounded-md border-2 border-gray-200"
                    placeholder={settings.random_seed}
                ></input>
            </div>

            <div className="flex flex-row justify-center items-center">
                <div className="text-center mr-2.5">Hiding time</div>
                <input
                    type="number"
                    min="1"
                    className="text-center px-1 py-1 m-5 rounded-md border-2 border-gray-200"
                    placeholder={settings.hiding_time_seconds}
                ></input>
            </div>

            <div className="flex flex-row justify-center items-center">
                <div className="text-center mr-2.5">Powerup Type</div>
                <select className="text-center px-1 py-1 m-5 rounded-md border-2 border-gray-200">
                    <option value='PingSeeker'>Ping Seeker</option>
                    <option value='PingAllSeekers'>Ping All Seekers</option>
                    <option value='ForcePingOther'>Force Ping Others</option>
                </select>
            </div>
        </div>
    );
}
