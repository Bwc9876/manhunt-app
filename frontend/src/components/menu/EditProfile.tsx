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
        <div className="w-full h-full flex flex-col items-center justify-center">
            <div className="flex flex-col items-center justify-center p-3.5 w-4/5">
                <input
                    className="input-field p-5 w-2/3 m-4.5"
                    placeholder={profile.display_name}
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                ></input>

                <button
                    className="btn-blue px-7 py-3 w-1/2"
                    onClick={() => {
                        onSaveProfile();
                    }}
                    disabled={newName.length === 0}
                >
                    Save
                </button>
            </div>
        </div>
    );
}
