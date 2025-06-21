import { commands, GameSettings } from "@/bindings";
import { unwrapResult } from "@/lib/result";
import React from "react";

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

function MainMenu({
    profilePromise,
    historyPromise
}: {
    profilePromise: ReturnType<typeof commands.getProfile>;
    historyPromise: ReturnType<typeof commands.listGameHistories>;
}) {
    const initialProfile = unwrapResult(React.use(profilePromise));
    const gameHistory = unwrapResult(React.use(historyPromise));
    const [profile, setProfile] = React.useState(initialProfile);
    const [newName, setName] = React.useState(initialProfile.display_name);
    const [roomCode, setRoomCode] = React.useState("");

    const onStartGame = async (code: string | null) => {
        if (code) {
            try {
                const validCode = unwrapResult(await commands.checkRoomCode(code));
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
        unwrapResult(await commands.updateProfile({ ...profile, display_name: newName }));
        setProfile((p) => {
            return { ...p, display_name: newName };
        });
    };

    return (
        <>
            {profile.pfp_base64 && (
                <img src={profile.pfp_base64} alt={`${profile.display_name}'s Profile Picture`} />
            )}
            <h2>Welcome, {profile.display_name}</h2>
            <hr />
            <h3>Play</h3>
            <button onClick={() => onStartGame(null)}>Start Lobby</button>
            <div>
                <input
                    value={roomCode}
                    placeholder="Room Code"
                    onChange={(e) => setRoomCode(e.target.value)}
                />
                <button onClick={() => onStartGame(roomCode)} disabled={roomCode === ""}>
                    Join Lobby
                </button>
            </div>
            <hr />
            <h3>Edit Profile</h3>
            <input value={newName} onChange={(e) => setName(e.target.value)} />
            <button onClick={onSaveProfile}>Save</button>
            <hr />
            <h3>Previous Games</h3>
            <ul>
                {gameHistory.map((time) => (
                    <li key={time}>{time}</li>
                ))}
            </ul>
        </>
    );
}

export default function MenuScreen() {
    const profilePromise = commands.getProfile();
    const previousGamesPromise = commands.listGameHistories();

    return (
        <React.Suspense fallback={<p>Loading profile</p>}>
            <MainMenu profilePromise={profilePromise} historyPromise={previousGamesPromise} />
        </React.Suspense>
    );
}
