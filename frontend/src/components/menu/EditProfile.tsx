import React, { useState } from "react";
import { sharedSwrConfig } from "@/lib/hooks";
import useSWR from "swr";
import { commands } from "@/bindings";

export default function EditProfile() {
    const { data: profile, mutate: setProfile } = useSWR(
        "fetch-profile",
        commands.getProfile,
        sharedSwrConfig
    );

    const [newName, setNewName] = useState(profile.display_name);

    const onSaveProfile = async () => {
        await commands.updateProfile({ ...profile, display_name: newName });
        setProfile({ ...profile, display_name: newName });
    };

    return (
        <div className="flex flex-col items-center justify-center p-3.5">
            <input
                className="text-center px-2 py-3 m-5 rounded-md border-2 border-gray-200"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
            ></input>

            <button
                className="bg-blue-500 px-7 py-2 height text-white p-3.5 rounded-md"
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
