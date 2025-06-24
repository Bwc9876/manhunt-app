import { commands, GameSettings } from "@/bindings";
import { sharedSwrConfig } from "@/lib/hooks";
import React from "react";
import useSWR from "swr";

// Temp settings for now.
const settings: GameSettings = {
    random_seed: 21341234,
    hiding_time_seconds: 10,
    ping_start: "Instant",
    ping_minutes_interval: 1,
    powerup_start: "Instant",
    powerup_chance: 60,
    powerup_minutes_cooldown: 1,
    powerup_locations: [
        {
            lat: 0,
            long: 0,
            heading: null
        }
    ]
};

export default function MenuScreen() {
    const [roomCode, setRoomCode] = React.useState("");
    const [newName, setName] = React.useState("");

    const { data: profile, mutate: setProfile } = useSWR(
        "fetch-profile",
        commands.getProfile,
        sharedSwrConfig
    );
    const { data: gameHistory } = useSWR(
        "list-game-history",
        commands.listGameHistories,
        sharedSwrConfig
    );

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

    const onSaveProfile = async () => {
        await commands.updateProfile({ ...profile, display_name: newName });
        setProfile({ ...profile, display_name: newName });
    };

    return (
        <>
            <div className="flex flex-col items-center font-sans">
                {profile.pfp_base64 && (
                <img src={profile.pfp_base64} alt={`${profile.display_name}'s Profile Picture`} />
                )}

                <h2 className="text-center text-lg font-semibold p-5">Welcome, {profile.display_name}</h2>
                
                <div className="flex flex-col items-center p-3.5">
                    <input className="text-center px- py-3 m-5 rounded-md border-2 border-gray-200" placeholder="Room Code" onChange={(e) => setRoomCode(e.target.value)}></input>
                    <button className="bg-blue-500 px-7 py-2 height text-white p-3.5 rounded-md" onClick={() => {onStartGame(roomCode)}} disabled={roomCode.length <= 0}>Join</button>
                </div>


                <h3>Edit Profile</h3>
                <input
                    placeholder={profile.display_name}
                    value={newName}
                    onChange={(e) => setName(e.target.value)}
                />
                <button onClick={onSaveProfile}>Save</button>

                <h3>Previous Games</h3>
                <ul>
                    {gameHistory.map((time) => (
                        <li key={time}>{time}</li>
                    ))}
                </ul>
            </div>
        </>
    );
}
