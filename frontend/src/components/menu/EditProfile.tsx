import React, { useState } from "react";
import { commands, PlayerProfile } from "@/bindings";
import { KeyedMutator } from "swr";

export default function EditProfile({
    profile,
    setProfile
}: {
    profile: PlayerProfile;
    setProfile: KeyedMutator<PlayerProfile>;
}) {
    const [newName, setNewName] = useState("");

    const onSaveProfile = async () => {
        await commands.updateProfile({ ...profile, display_name: newName });
        setProfile({ ...profile, display_name: newName });
    };

    return (
        <div className="flex flex-col items-center justify-center p-3.5">
            <input
                className="input-field px-2 py-3"
                placeholder={profile.display_name}
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
            ></input>

            <button
                className="btn-blue px-7 py-3"
                onClick={() => {
                    onSaveProfile();
                }}
                disabled={newName.length === 0}
            >
                Save
            </button>
        </div>
    );
}
